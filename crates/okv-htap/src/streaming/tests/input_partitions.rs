//! Independent input-shape oracle, frozen before the implementation change.
//! Exact rows are authored separately from the candidate state machine.

use super::{
    base_input_schema, decode_snapshot_batch, partition_watermarks, tail_batch, tail_input_schema,
    SnapshotRow, StreamExecutionStats, StreamingOverlayMode, StreamingSnapshotOverlayExec,
    StreamingZebraSnapshotTableProvider, TailFixtureRow, ANALYTICAL_COVERAGE, TARGET_VERSION,
    WEST_WATERMARK,
};
use arrow::array::{ArrayRef, StringArray, UInt32Array, UInt64Array};
use arrow::record_batch::RecordBatch;
use datafusion::catalog::TableProvider;
use datafusion::common::{DataFusionError, Result};
use datafusion::datasource::MemTable;
use datafusion::execution::TaskContext;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, PlanProperties, SendableRecordBatchStream,
};
use datafusion::prelude::{SessionConfig, SessionContext};
use futures_util::TryStreamExt;
use std::fmt::Formatter;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug)]
struct CountedExec {
    input: Arc<dyn ExecutionPlan>,
    executions: Arc<AtomicUsize>,
}
impl DisplayAs for CountedExec {
    fn fmt_as(&self, _: DisplayFormatType, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "CountedExec")
    }
}
impl ExecutionPlan for CountedExec {
    fn name(&self) -> &'static str {
        "CountedExec"
    }
    fn properties(&self) -> &Arc<PlanProperties> {
        self.input.properties()
    }
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        vec![&self.input]
    }
    fn with_new_children(
        self: Arc<Self>,
        mut children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        assert_eq!(children.len(), 1);
        Ok(Arc::new(Self {
            input: children.pop().expect("one child"),
            executions: Arc::clone(&self.executions),
        }))
    }
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        self.input.execute(partition, context)
    }
}
fn row(id: u64, amount_cents: u64, priority: u8) -> SnapshotRow {
    SnapshotRow {
        id,
        status: "OPEN".to_owned(),
        partition: "west".to_owned(),
        amount_cents,
        priority,
    }
}
fn base_batch(rows: &[SnapshotRow]) -> RecordBatch {
    RecordBatch::try_new(
        base_input_schema(),
        vec![
            Arc::new(UInt64Array::from(
                rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|row| row.status.as_str()),
            )),
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|row| row.partition.as_str()),
            )),
            Arc::new(UInt64Array::from(
                rows.iter().map(|row| row.amount_cents).collect::<Vec<_>>(),
            )),
            Arc::new(UInt32Array::from(vec![1; rows.len()])),
            Arc::new(UInt64Array::from(vec![WEST_WATERMARK; rows.len()])),
        ],
    )
    .expect("valid base fixture")
}
fn effect(id: u64, operation: &'static str, amount: Option<u64>) -> TailFixtureRow {
    TailFixtureRow {
        id,
        version: TARGET_VERSION,
        operation,
        previous_partition: (operation != "INSERT").then_some("west"),
        status: amount.map(|_| "OPEN"),
        partition: amount.map(|_| "west"),
        amount_cents: amount,
        priority: amount.map(|_| 1),
    }
}
fn tables(
    base_partitions: Vec<Vec<RecordBatch>>,
    tail_partitions: Vec<Vec<RecordBatch>>,
) -> (Arc<dyn TableProvider>, Arc<dyn TableProvider>) {
    (
        Arc::new(MemTable::try_new(base_input_schema(), base_partitions).expect("base MemTable")),
        Arc::new(MemTable::try_new(tail_input_schema(), tail_partitions).expect("tail MemTable")),
    )
}
fn refusal_message(base_count: usize, tail_count: usize) -> String {
    format!("StreamingSnapshotOverlayExec requires single-partition inputs: base partitions={base_count}, tail partitions={tail_count}")
}
fn assert_refusal(error: DataFusionError, base_count: usize, tail_count: usize) {
    let DataFusionError::Execution(message) = error else {
        panic!("expected explicit unsupported-input execution error, received {error}");
    };
    assert_eq!(message, refusal_message(base_count, tail_count));
}
async fn exact_or_refused_before_execution(
    base_partitions: Vec<Vec<RecordBatch>>,
    tail_partitions: Vec<Vec<RecordBatch>>,
    expected: Vec<SnapshotRow>,
) {
    let base_count = base_partitions.len();
    let tail_count = tail_partitions.len();
    let (base, tail) = tables(base_partitions, tail_partitions);
    let context = SessionContext::new_with_config(SessionConfig::new().with_target_partitions(1));
    let state = context.state();
    let base_executions = Arc::new(AtomicUsize::new(0));
    let tail_executions = Arc::new(AtomicUsize::new(0));
    let base = Arc::new(CountedExec {
        input: base.scan(&state, None, &[], None).await.expect("base plan"),
        executions: Arc::clone(&base_executions),
    });
    let tail = Arc::new(CountedExec {
        input: tail.scan(&state, None, &[], None).await.expect("tail plan"),
        executions: Arc::clone(&tail_executions),
    });
    let overlay = StreamingSnapshotOverlayExec::new(
        base,
        tail,
        TARGET_VERSION,
        WEST_WATERMARK,
        partition_watermarks(),
        StreamingOverlayMode::Correct,
        Arc::new(StreamExecutionStats::default()),
    );
    let result = match overlay.execute(0, context.task_ctx()) {
        Ok(stream) => stream.try_collect::<Vec<_>>().await,
        Err(error) => Err(error),
    };
    match result {
        Ok(batches) => {
            let actual = batches
                .iter()
                .flat_map(|batch| decode_snapshot_batch(batch).expect("output rows"))
                .collect::<Vec<_>>();
            assert_eq!(
                actual, expected,
                "nonzero input partitions must not be silently omitted"
            );
        }
        Err(error) => {
            assert_refusal(error, base_count, tail_count);
            assert_eq!(
                base_executions.load(Ordering::SeqCst),
                0,
                "base executed before refusal"
            );
            assert_eq!(
                tail_executions.load(Ordering::SeqCst),
                0,
                "tail executed before refusal"
            );
        }
    }
}
#[tokio::test]
async fn partition_one_insert_is_exact_or_refused() {
    exact_or_refused_before_execution(
        vec![vec![base_batch(&[row(1, 100, 0)])]],
        vec![
            vec![],
            vec![tail_batch(&[effect(2, "INSERT", Some(200))]).unwrap()],
        ],
        vec![row(1, 100, 0), row(2, 200, 1)],
    )
    .await;
}
#[tokio::test]
async fn partition_one_update_is_exact_or_refused() {
    exact_or_refused_before_execution(
        vec![vec![base_batch(&[row(1, 100, 0)])]],
        vec![
            vec![],
            vec![tail_batch(&[effect(1, "UPDATE", Some(110))]).unwrap()],
        ],
        vec![row(1, 110, 1)],
    )
    .await;
}
#[tokio::test]
async fn partition_one_delete_is_exact_or_refused() {
    exact_or_refused_before_execution(
        vec![vec![base_batch(&[row(1, 100, 0)])]],
        vec![
            vec![],
            vec![tail_batch(&[effect(1, "DELETE", None)]).unwrap()],
        ],
        vec![],
    )
    .await;
}
#[tokio::test]
async fn partition_one_base_is_exact_or_refused() {
    exact_or_refused_before_execution(
        vec![vec![], vec![base_batch(&[row(1, 100, 0)])]],
        vec![vec![]],
        vec![row(1, 100, 0)],
    )
    .await;
}
#[tokio::test]
async fn both_multipartition_inputs_are_exact_or_refused() {
    exact_or_refused_before_execution(
        vec![
            vec![base_batch(&[row(1, 100, 0)])],
            vec![base_batch(&[row(2, 200, 0)])],
        ],
        vec![
            vec![],
            vec![tail_batch(&[
                effect(1, "UPDATE", Some(110)),
                effect(2, "DELETE", None),
                effect(3, "INSERT", Some(300)),
            ])
            .unwrap()],
        ],
        vec![row(1, 110, 1), row(3, 300, 1)],
    )
    .await;
}
async fn planned_rows(target_partitions: usize, split_tail: bool) -> Result<Vec<SnapshotRow>> {
    let effects = tail_batch(&[
        effect(1, "UPDATE", Some(110)),
        effect(2, "DELETE", None),
        effect(3, "INSERT", Some(300)),
    ])
    .unwrap();
    let tail_partitions = if split_tail {
        vec![vec![], vec![effects]]
    } else {
        vec![vec![effects]]
    };
    let (base, tail) = tables(
        vec![vec![base_batch(&[row(1, 100, 0), row(2, 200, 0)])]],
        tail_partitions,
    );
    let context = SessionContext::new_with_config(
        SessionConfig::new().with_target_partitions(target_partitions),
    );
    context.register_table(
        "orders",
        Arc::new(StreamingZebraSnapshotTableProvider::new(
            base,
            tail,
            TARGET_VERSION,
            partition_watermarks(),
            ANALYTICAL_COVERAGE,
            StreamingOverlayMode::Correct,
            Arc::new(StreamExecutionStats::default()),
        )),
    )?;
    let dataframe = context.sql("SELECT id, status, partition, amount_cents, priority FROM orders ORDER BY id, partition").await?;
    let batches = dataframe.collect().await?;
    Ok(batches
        .iter()
        .flat_map(|batch| decode_snapshot_batch(batch).expect("output rows"))
        .collect())
}
#[tokio::test]
async fn sql_planner_multipartition_tail_is_exact_or_explicitly_refused() {
    match planned_rows(2, true).await {
        Ok(actual) => assert_eq!(actual, vec![row(1, 110, 1), row(3, 300, 1)]),
        Err(error) => assert!(
            error.to_string().contains(&refusal_message(1, 2)),
            "unexpected planner failure: {error}"
        ),
    }
}
#[tokio::test]
async fn sql_planner_single_partition_preserves_exact_snapshot() {
    assert_eq!(
        planned_rows(1, false)
            .await
            .expect("single-partition query"),
        vec![row(1, 110, 1), row(3, 300, 1)]
    );
}
