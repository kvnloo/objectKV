use super::super::{
    base_input_schema, tail_batch, tail_input_schema, SnapshotRow, StreamExecutionStats,
    StreamingMergeState, StreamingOverlayMode, TailFixtureRow, MAXIMUM_GROUP_ROWS,
};
use arrow::array::{ArrayRef, StringArray, UInt32Array, UInt64Array};
use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use datafusion::common::DataFusionError;
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::SendableRecordBatchStream;
use futures_util::stream;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Poll;

const LOWER_BOUND: u64 = 5;
const TARGET: u64 = 32;

// Each poll supplies one real input row. An error after the permitted prefix
// makes an extra request observable without allocating a large fixture.
fn counted_stream(
    schema: SchemaRef,
    batches: Vec<RecordBatch>,
    poison_next_poll: bool,
) -> (SendableRecordBatchStream, Arc<AtomicUsize>) {
    let polls = Arc::new(AtomicUsize::new(0));
    let stream_polls = Arc::clone(&polls);
    let mut batches = batches.into_iter();
    let input = stream::poll_fn(move |_| {
        stream_polls.fetch_add(1, Ordering::Relaxed);
        Poll::Ready(match batches.next() {
            Some(batch) => Some(Ok(batch)),
            None if poison_next_poll => Some(Err(DataFusionError::Execution(
                "polled beyond the first overflowing group row".to_owned(),
            ))),
            None => None,
        })
    });
    (
        Box::pin(RecordBatchStreamAdapter::new(schema, input)),
        polls,
    )
}

fn base_rows(id: u64, count: usize) -> Vec<SnapshotRow> {
    (0..count)
        .map(|index| SnapshotRow {
            id,
            status: "OPEN".to_owned(),
            partition: format!("p{index:02}"),
            amount_cents: u64::try_from(index).expect("small fixture index"),
            priority: 0,
        })
        .collect()
}

fn base_batches(rows: &[SnapshotRow]) -> Vec<RecordBatch> {
    rows.iter()
        .map(|row| {
            RecordBatch::try_new(
                base_input_schema(),
                vec![
                    Arc::new(UInt64Array::from(vec![row.id])) as ArrayRef,
                    Arc::new(StringArray::from(vec![row.status.as_str()])),
                    Arc::new(StringArray::from(vec![row.partition.as_str()])),
                    Arc::new(UInt64Array::from(vec![row.amount_cents])),
                    Arc::new(UInt32Array::from(vec![1])),
                    Arc::new(UInt64Array::from(vec![LOWER_BOUND])),
                ],
            )
            .expect("one-row base batch")
        })
        .collect()
}

fn tail_rows(id: u64, versions: impl IntoIterator<Item = u64>) -> Vec<TailFixtureRow> {
    versions
        .into_iter()
        .map(|version| TailFixtureRow {
            id,
            version,
            operation: "UPDATE",
            previous_partition: Some("west"),
            status: Some("CLOSED"),
            partition: Some("west"),
            amount_cents: Some(version),
            priority: Some(1),
        })
        .collect()
}

fn tail_batches(rows: &[TailFixtureRow]) -> Vec<RecordBatch> {
    rows.iter()
        .map(|row| tail_batch(&[*row]).expect("one-row tail batch"))
        .collect()
}

fn merge(
    base: SendableRecordBatchStream,
    tail: SendableRecordBatchStream,
) -> (StreamingMergeState, Arc<StreamExecutionStats>) {
    let watermarks = (0..=MAXIMUM_GROUP_ROWS)
        .map(|index| (format!("p{index:02}"), LOWER_BOUND))
        .chain(std::iter::once(("west".to_owned(), LOWER_BOUND)))
        .collect::<BTreeMap<_, _>>();
    let stats = Arc::new(StreamExecutionStats::default());
    (
        StreamingMergeState::new(
            base,
            tail,
            TARGET,
            LOWER_BOUND,
            watermarks,
            StreamingOverlayMode::Correct,
            Arc::clone(&stats),
        ),
        stats,
    )
}

fn assert_group_limit(error: DataFusionError, stats: &StreamExecutionStats) {
    let DataFusionError::ResourcesExhausted(message) = error else {
        panic!("expected early resource refusal, received {error}");
    };
    assert_eq!(message, "logical id 1 group has 17 rows; limit is 16");
    assert_eq!(stats.maximum_group_rows.load(Ordering::Relaxed), 17);
}

fn latest_row(id: u64, version: u64) -> SnapshotRow {
    SnapshotRow {
        id,
        status: "CLOSED".to_owned(),
        partition: "west".to_owned(),
        amount_cents: version,
        priority: 1,
    }
}

#[tokio::test]
async fn base_group_refuses_row_17_before_polling_row_18() {
    let (base, base_polls) =
        counted_stream(base_input_schema(), base_batches(&base_rows(1, 17)), true);
    let (tail, _) = counted_stream(tail_input_schema(), Vec::new(), false);
    let (mut state, stats) = merge(base, tail);
    assert_group_limit(
        state.next_group().await.expect_err("oversized base group"),
        &stats,
    );
    assert_eq!(base_polls.load(Ordering::Relaxed), 17);
}

#[tokio::test]
async fn tail_group_refuses_row_17_before_polling_row_18() {
    let (base, _) = counted_stream(base_input_schema(), Vec::new(), false);
    let (tail, tail_polls) = counted_stream(
        tail_input_schema(),
        tail_batches(&tail_rows(1, 6..=22)),
        true,
    );
    let (mut state, stats) = merge(base, tail);
    assert_group_limit(
        state.next_group().await.expect_err("oversized tail group"),
        &stats,
    );
    assert_eq!(tail_polls.load(Ordering::Relaxed), 17);
}

#[tokio::test]
async fn mixed_group_refuses_combined_row_17_before_polling_row_18() {
    let (base, base_polls) =
        counted_stream(base_input_schema(), base_batches(&base_rows(1, 8)), false);
    let (tail, tail_polls) = counted_stream(
        tail_input_schema(),
        tail_batches(&tail_rows(1, 6..=14)),
        true,
    );
    let (mut state, stats) = merge(base, tail);
    assert_group_limit(
        state.next_group().await.expect_err("oversized mixed group"),
        &stats,
    );
    assert_eq!(base_polls.load(Ordering::Relaxed), 9);
    assert_eq!(tail_polls.load(Ordering::Relaxed), 9);
}

#[tokio::test]
async fn mixed_group_counts_tail_lookahead_before_another_base_poll() {
    let (base, base_polls) =
        counted_stream(base_input_schema(), base_batches(&base_rows(1, 16)), true);
    let (tail, tail_polls) =
        counted_stream(tail_input_schema(), tail_batches(&tail_rows(1, [6])), true);
    let (mut state, stats) = merge(base, tail);
    assert_group_limit(
        state.next_group().await.expect_err("oversized mixed group"),
        &stats,
    );
    assert_eq!(base_polls.load(Ordering::Relaxed), 16);
    assert_eq!(tail_polls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn exactly_16_base_rows_pass_and_the_next_id_gets_a_fresh_bound() {
    let first = base_rows(1, 16);
    let second = base_rows(2, 16);
    let rows = [first.clone(), second.clone()].concat();
    let (base, _) = counted_stream(base_input_schema(), base_batches(&rows), false);
    let (tail, _) = counted_stream(tail_input_schema(), Vec::new(), false);
    let (mut state, stats) = merge(base, tail);
    assert_eq!(state.next_group().await.expect("first group"), Some(first));
    assert_eq!(
        state.next_group().await.expect("second group"),
        Some(second)
    );
    assert_eq!(state.next_group().await.expect("end of inputs"), None);
    assert_eq!(stats.maximum_group_rows.load(Ordering::Relaxed), 16);
}

#[tokio::test]
async fn exactly_16_tail_rows_pass_and_the_next_id_gets_a_fresh_bound() {
    let rows = [tail_rows(1, 6..=21), tail_rows(2, 6..=21)].concat();
    let (base, _) = counted_stream(base_input_schema(), Vec::new(), false);
    let (tail, _) = counted_stream(tail_input_schema(), tail_batches(&rows), false);
    let (mut state, stats) = merge(base, tail);
    assert_eq!(
        state.next_group().await.expect("first group"),
        Some(vec![latest_row(1, 21)])
    );
    assert_eq!(
        state.next_group().await.expect("second group"),
        Some(vec![latest_row(2, 21)])
    );
    assert_eq!(state.next_group().await.expect("end of inputs"), None);
    assert_eq!(stats.maximum_group_rows.load(Ordering::Relaxed), 16);
}

#[tokio::test]
async fn exactly_16_mixed_rows_suppress_base_and_emit_the_latest_tail() {
    let (base, _) = counted_stream(base_input_schema(), base_batches(&base_rows(1, 8)), false);
    let (tail, _) = counted_stream(
        tail_input_schema(),
        tail_batches(&tail_rows(1, 6..=13)),
        false,
    );
    let (mut state, stats) = merge(base, tail);
    assert_eq!(
        state.next_group().await.expect("mixed group"),
        Some(vec![latest_row(1, 13)])
    );
    assert_eq!(state.next_group().await.expect("end of inputs"), None);
    assert_eq!(stats.maximum_group_rows.load(Ordering::Relaxed), 16);
}

#[tokio::test]
async fn excluded_versions_do_not_count_toward_the_group_bound() {
    let rows = [
        tail_rows(1, [LOWER_BOUND]),
        tail_rows(1, 6..=21),
        tail_rows(1, 33..=49),
        tail_rows(2, [TARGET]),
    ]
    .concat();
    let (base, _) = counted_stream(base_input_schema(), Vec::new(), false);
    let (tail, _) = counted_stream(tail_input_schema(), tail_batches(&rows), false);
    let (mut state, stats) = merge(base, tail);
    assert_eq!(
        state.next_group().await.expect("visible group"),
        Some(vec![latest_row(1, 21)])
    );
    assert_eq!(
        state.next_group().await.expect("next visible id"),
        Some(vec![latest_row(2, TARGET)])
    );
    assert_eq!(state.next_group().await.expect("end of inputs"), None);
    assert_eq!(stats.maximum_group_rows.load(Ordering::Relaxed), 16);
}

#[tokio::test]
async fn exactly_16_mixed_rows_still_apply_delete() {
    let mut rows = tail_rows(1, 6..=13);
    let delete = rows.last_mut().expect("tail is nonempty");
    delete.operation = "DELETE";
    delete.status = None;
    delete.partition = None;
    delete.amount_cents = None;
    delete.priority = None;
    let (base, _) = counted_stream(base_input_schema(), base_batches(&base_rows(1, 8)), false);
    let (tail, _) = counted_stream(tail_input_schema(), tail_batches(&rows), false);
    let (mut state, _) = merge(base, tail);
    assert_eq!(
        state.next_group().await.expect("deleted group"),
        Some(vec![])
    );
    assert_eq!(state.next_group().await.expect("end of inputs"), None);
}

#[tokio::test]
async fn in_bound_duplicate_base_representations_still_fail() {
    let row = base_rows(1, 1).pop().expect("one base row");
    let (base, _) = counted_stream(
        base_input_schema(),
        base_batches(&[row.clone(), row]),
        false,
    );
    let (tail, _) = counted_stream(tail_input_schema(), Vec::new(), false);
    let (mut state, _) = merge(base, tail);
    let error = state.next_group().await.expect_err("duplicate base key");
    assert!(matches!(error, DataFusionError::Execution(_)));
    assert!(error.to_string().contains("duplicate base representation"));
}

#[tokio::test]
async fn in_bound_cross_batch_order_validation_still_fails() {
    let mut rows = base_rows(1, 2);
    rows.reverse();
    let (base, _) = counted_stream(base_input_schema(), base_batches(&rows), false);
    let (tail, _) = counted_stream(tail_input_schema(), Vec::new(), false);
    let (mut state, _) = merge(base, tail);
    let error = state.next_group().await.expect_err("unordered base");
    assert!(error.to_string().contains("base input is not ordered"));

    let (base, _) = counted_stream(base_input_schema(), Vec::new(), false);
    let (tail, _) = counted_stream(
        tail_input_schema(),
        tail_batches(&tail_rows(1, [7, 6])),
        false,
    );
    let (mut state, _) = merge(base, tail);
    let error = state.next_group().await.expect_err("unordered tail");
    assert!(error.to_string().contains("tail input is not ordered"));
}
