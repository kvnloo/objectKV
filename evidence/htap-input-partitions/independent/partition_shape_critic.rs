//! Independent regression witness. Credited to Wiley Jones.
//! Frozen separately from the implementation and its primary tests.

#[allow(dead_code)]
mod internal_adapter {
    include!("../src/streaming.rs");

    #[cfg(test)]
    mod independent_partition_critic {
        use super::*;
        use datafusion::physical_plan::{collect, ExecutionPlanProperties};
        use std::sync::atomic::AtomicUsize;

        #[derive(Debug)]
        struct ShapeProbe {
            properties: Arc<PlanProperties>,
            calls: Arc<AtomicUsize>,
        }

        impl DisplayAs for ShapeProbe {
            fn fmt_as(&self, _: DisplayFormatType, f: &mut Formatter<'_>) -> std::fmt::Result {
                write!(f, "IndependentShapeProbe")
            }
        }

        impl ExecutionPlan for ShapeProbe {
            fn name(&self) -> &'static str {
                "IndependentShapeProbe"
            }

            fn properties(&self) -> &Arc<PlanProperties> {
                &self.properties
            }

            fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
                vec![]
            }

            fn with_new_children(
                self: Arc<Self>,
                children: Vec<Arc<dyn ExecutionPlan>>,
            ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
                assert!(children.is_empty());
                Ok(self)
            }

            fn execute(
                &self,
                _: usize,
                _: Arc<TaskContext>,
            ) -> DataFusionResult<SendableRecordBatchStream> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(Box::pin(RecordBatchStreamAdapter::new(
                    self.schema(),
                    stream::empty(),
                )))
            }
        }

        fn probe(schema: SchemaRef, count: usize) -> (Arc<dyn ExecutionPlan>, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            let plan = ShapeProbe {
                properties: Arc::new(PlanProperties::new(
                    EquivalenceProperties::new(schema),
                    Partitioning::UnknownPartitioning(count),
                    EmissionType::Incremental,
                    Boundedness::Bounded,
                )),
                calls: Arc::clone(&calls),
            };
            (Arc::new(plan), calls)
        }

        fn overlay(
            base: Arc<dyn ExecutionPlan>,
            tail: Arc<dyn ExecutionPlan>,
            mode: StreamingOverlayMode,
        ) -> Arc<StreamingSnapshotOverlayExec> {
            Arc::new(StreamingSnapshotOverlayExec::new(
                base,
                tail,
                12,
                5,
                BTreeMap::from([("west".to_owned(), 5)]),
                mode,
                Arc::new(StreamExecutionStats::default()),
            ))
        }

        fn refusal(base_count: usize, tail_count: usize, mode: StreamingOverlayMode) {
            let (base, base_calls) = probe(base_input_schema(), base_count);
            let (tail, tail_calls) = probe(tail_input_schema(), tail_count);
            let plan = overlay(base, tail, mode);
            let result = plan.execute(0, Arc::new(TaskContext::default()));
            assert_eq!(
                (base_calls.load(Ordering::SeqCst), tail_calls.load(Ordering::SeqCst)),
                (0, 0),
                "Both shapes must be validated before either child executes: {base_count}/{tail_count}"
            );
            match result {
                Err(DataFusionError::Execution(message)) => assert_eq!(
                    message,
                    format!("StreamingSnapshotOverlayExec requires single-partition inputs: base partitions={base_count}, tail partitions={tail_count}")
                ),
                Err(other) => panic!("Expected exact execution refusal, got {other}"),
                Ok(_) => panic!("Unsupported shape {base_count}/{tail_count} was admitted"),
            }
        }

        macro_rules! refusal_test {
            ($name:ident, $base:expr, $tail:expr) => {
                #[test]
                fn $name() {
                    refusal($base, $tail, StreamingOverlayMode::Correct);
                    refusal($base, $tail, StreamingOverlayMode::MaterializeInputs);
                }
            };
        }

        refusal_test!(reject_zero_zero, 0, 0);
        refusal_test!(reject_zero_one, 0, 1);
        refusal_test!(reject_one_zero_before_base_starts, 1, 0);
        refusal_test!(reject_zero_two, 0, 2);
        refusal_test!(reject_two_zero, 2, 0);
        refusal_test!(reject_two_one, 2, 1);
        refusal_test!(reject_one_two_before_base_starts, 1, 2);
        refusal_test!(reject_two_two, 2, 2);

        #[tokio::test]
        async fn empty_single_partition_inputs_are_valid_in_both_modes() {
            for mode in [
                StreamingOverlayMode::Correct,
                StreamingOverlayMode::MaterializeInputs,
            ] {
                let (base, base_calls) = probe(base_input_schema(), 1);
                let (tail, tail_calls) = probe(tail_input_schema(), 1);
                let batches = collect(overlay(base, tail, mode), Arc::new(TaskContext::default()))
                    .await
                    .expect("one empty partition is a valid stream");
                assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 0);
                assert_eq!(base_calls.load(Ordering::SeqCst), 1);
                assert_eq!(tail_calls.load(Ordering::SeqCst), 1);
            }
        }

        #[test]
        fn optimizer_child_replacement_cannot_bypass_guard() {
            let (base, _) = probe(base_input_schema(), 1);
            let (tail, _) = probe(tail_input_schema(), 1);
            let original = overlay(base, tail, StreamingOverlayMode::Correct);
            let (replacement_base, base_calls) = probe(base_input_schema(), 1);
            let (replacement_tail, tail_calls) = probe(tail_input_schema(), 2);
            let replaced = original
                .with_new_children(vec![replacement_base, replacement_tail])
                .expect("child replacement is plan construction, not execution");
            let outcome = replaced.execute(0, Arc::new(TaskContext::default()));
            assert!(matches!(outcome, Err(DataFusionError::Execution(_))));
            assert_eq!(base_calls.load(Ordering::SeqCst), 0);
            assert_eq!(tail_calls.load(Ordering::SeqCst), 0);
        }

        fn independent_base() -> RecordBatch {
            RecordBatch::try_new(
                base_input_schema(),
                vec![
                    Arc::new(UInt64Array::from(vec![10, 20])),
                    Arc::new(StringArray::from(vec!["OPEN", "OPEN"])),
                    Arc::new(StringArray::from(vec!["west", "west"])),
                    Arc::new(UInt64Array::from(vec![100, 200])),
                    Arc::new(UInt32Array::from(vec![1, 1])),
                    Arc::new(UInt64Array::from(vec![5, 5])),
                ],
            )
            .unwrap()
        }

        fn effect(id: u64, operation: &'static str, amount: Option<u64>) -> TailFixtureRow {
            TailFixtureRow {
                id,
                version: 12,
                operation,
                previous_partition: if operation == "INSERT" {
                    None
                } else {
                    Some("west")
                },
                status: amount.map(|_| "OPEN"),
                partition: amount.map(|_| "west"),
                amount_cents: amount,
                priority: amount.map(|_| 0),
            }
        }

        async fn planner_context(target_partitions: usize) -> SessionContext {
            let context = SessionContext::new_with_config(
                SessionConfig::new()
                    .with_target_partitions(target_partitions)
                    .with_batch_size(2),
            );
            let base: Arc<dyn TableProvider> = Arc::new(
                MemTable::try_new(base_input_schema(), vec![vec![independent_base()]]).unwrap(),
            );
            let tail: Arc<dyn TableProvider> = Arc::new(
                MemTable::try_new(
                    tail_input_schema(),
                    vec![vec![
                        tail_batch(&[effect(10, "UPDATE", Some(110)), effect(20, "DELETE", None)])
                            .unwrap(),
                        tail_batch(&[
                            effect(30, "INSERT", Some(300)),
                            effect(40, "INSERT", Some(400)),
                        ])
                        .unwrap(),
                    ]],
                )
                .unwrap(),
            );
            for provider in [&base, &tail] {
                let raw = provider
                    .scan(&context.state(), None, &[], None)
                    .await
                    .unwrap();
                assert_eq!(
                    raw.output_partitioning().partition_count(),
                    1,
                    "Fixture must start with one partition per input"
                );
            }
            context
                .register_table(
                    "probe_orders",
                    Arc::new(StreamingZebraSnapshotTableProvider::new(
                        base,
                        tail,
                        12,
                        BTreeMap::from([("west".to_owned(), 5)]),
                        12,
                        StreamingOverlayMode::Correct,
                        Arc::new(StreamExecutionStats::default()),
                    )),
                )
                .unwrap();
            context
        }

        fn overlay_shape(plan: &Arc<dyn ExecutionPlan>) -> Option<(usize, usize)> {
            if plan.name() == "StreamingSnapshotOverlayExec" {
                let children = plan.children();
                return Some((
                    children[0].output_partitioning().partition_count(),
                    children[1].output_partitioning().partition_count(),
                ));
            }
            plan.children().into_iter().find_map(overlay_shape)
        }

        #[tokio::test]
        async fn real_optimizer_expands_originally_single_partition_tail_and_fails_closed() {
            let context = planner_context(2).await;
            let plan = context
                .sql("SELECT * FROM probe_orders")
                .await
                .unwrap()
                .create_physical_plan()
                .await
                .unwrap();
            let shape =
                overlay_shape(&plan).expect("internal overlay survives normal SQL planning");
            assert_eq!(
                shape,
                (1, 2),
                "DataFusion 54 must actually create a second tail partition"
            );
            let result = collect(plan, context.task_ctx()).await;
            let expected = "StreamingSnapshotOverlayExec requires single-partition inputs: base partitions=1, tail partitions=2";
            match result {
                Err(error) => assert!(error.to_string().contains(expected), "{error}"),
                Ok(batches) => panic!(
                    "Planner produced incomplete rows instead of refusing: {:?}",
                    canonical_rows(&batches)
                ),
            }
        }

        #[tokio::test]
        async fn independent_single_partition_update_delete_insert_result_is_exact() {
            let context = planner_context(1).await;
            let batches = context
                .sql("SELECT * FROM probe_orders")
                .await
                .unwrap()
                .collect()
                .await
                .expect("supported shape remains exact");
            let actual = canonical_rows(&batches).unwrap();
            assert_eq!(
                actual,
                vec![
                    "id=10|status=OPEN|partition=west|amount_cents=110|priority=0",
                    "id=30|status=OPEN|partition=west|amount_cents=300|priority=0",
                    "id=40|status=OPEN|partition=west|amount_cents=400|priority=0",
                ]
            );
        }
        #[tokio::test]
        async fn actual_parquet_files_produce_multiple_base_partitions_and_refuse() {
            let directory = tempfile::tempdir().unwrap();
            let rows = independent_base();
            write_parquet(&directory.path().join("first.parquet"), &rows.slice(0, 1)).unwrap();
            write_parquet(&directory.path().join("second.parquet"), &rows.slice(1, 1)).unwrap();
            let mut config = SessionConfig::new().with_target_partitions(2);
            config
                .options_mut()
                .execution
                .parquet
                .schema_force_view_types = false;
            let context = SessionContext::new_with_config(config);
            context
                .register_parquet(
                    "raw_parquet",
                    directory.path().to_str().unwrap(),
                    ParquetReadOptions::default(),
                )
                .await
                .unwrap();
            let base = context.table_provider("raw_parquet").await.unwrap();
            let raw = base.scan(&context.state(), None, &[], None).await.unwrap();
            assert_eq!(
                raw.output_partitioning().partition_count(),
                2,
                "Two real Parquet files must expose two execution partitions"
            );
            let raw_batches = collect(raw, context.task_ctx()).await.unwrap();
            assert_eq!(
                raw_batches.iter().map(RecordBatch::num_rows).sum::<usize>(),
                2
            );
            let tail = Arc::new(MemTable::try_new(tail_input_schema(), vec![vec![]]).unwrap());
            context
                .register_table(
                    "parquet_orders",
                    Arc::new(StreamingZebraSnapshotTableProvider::new(
                        base,
                        tail,
                        12,
                        BTreeMap::from([("west".to_owned(), 5)]),
                        12,
                        StreamingOverlayMode::Correct,
                        Arc::new(StreamExecutionStats::default()),
                    )),
                )
                .unwrap();
            let plan = context
                .sql("SELECT * FROM parquet_orders")
                .await
                .unwrap()
                .create_physical_plan()
                .await
                .unwrap();
            assert_eq!(overlay_shape(&plan), Some((2, 1)));
            match collect(plan, context.task_ctx()).await {
                Err(error) => assert!(error.to_string().contains(
                    "StreamingSnapshotOverlayExec requires single-partition inputs: base partitions=2, tail partitions=1"
                ), "{error}"),
                Ok(batches) => panic!("Parquet row was silently omitted: {:?}", canonical_rows(&batches)),
            }
        }
    }
}
