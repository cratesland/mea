use crate::channel::unbounded;
use crate::test_runtime;
use futures::StreamExt;

#[test]
fn test_unbounded() {
    let (tx, rx) = unbounded();

    test_runtime().block_on(async move {
        for i in 0..100 {
            let tx = tx.clone();
            tokio::spawn(async move {
                tx.send(i).await.unwrap();
            });
        }
        drop(tx);

        // let sum = rx
        //     .into_stream()
        //     .fold(0, |acc, i| async move { acc + i })
        //     .await;

        let mut sum = 0;
        while let Ok(i) = rx.recv().await {
            sum += i;
        }
        assert_eq!(sum, 4950);
    });
}
