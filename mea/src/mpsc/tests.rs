use std::time::Instant;
use crate::{mpsc, test_runtime};

#[test]
fn test_pressure() {
    let n = 1024 * 1024;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    test_runtime().block_on(async move {
        let start = Instant::now();
        tokio::spawn(async move {
            for i in 0..n {
                tx.send(i).unwrap();
            }
        });

        for i in 0..n {
            assert_eq!(rx.recv().await, Some(i));
        }
        println!("Elapsed: {:?}", start.elapsed());
    });
}