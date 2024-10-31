// Copyright 2024 tison <wander4096@gmail.com>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use tokio_test::assert_pending;
use tokio_test::assert_ready;
use tokio_test::task::spawn;

use crate::barrier::Barrier;

struct IsSend<T: Send>(T);

#[test]
fn barrier_future_is_send() {
    let b = Barrier::new(0);
    let _ = IsSend(b.wait());
}

#[test]
fn zero_does_not_block() {
    let b = Barrier::new(0);

    {
        let mut w = spawn(b.wait());
        let wr = assert_ready!(w.poll());
        assert!(wr);
    }
    {
        let mut w = spawn(b.wait());
        let wr = assert_ready!(w.poll());
        assert!(wr);
    }
}

#[test]
fn single() {
    let b = Barrier::new(1);

    {
        let mut w = spawn(b.wait());
        let wr = assert_ready!(w.poll());
        assert!(wr);
    }
    {
        let mut w = spawn(b.wait());
        let wr = assert_ready!(w.poll());
        assert!(wr);
    }
    {
        let mut w = spawn(b.wait());
        let wr = assert_ready!(w.poll());
        assert!(wr);
    }
}

#[test]
fn tango() {
    let b = Barrier::new(2);

    let mut w1 = spawn(b.wait());
    assert_pending!(w1.poll());

    let mut w2 = spawn(b.wait());
    let wr2 = assert_ready!(w2.poll());
    let wr1 = assert_ready!(w1.poll());

    assert!(wr1 || wr2);
    assert!(!(wr1 && wr2));
}

#[test]
fn lots() {
    let b = Barrier::new(100);

    for _ in 0..10 {
        let mut wait = Vec::new();
        for _ in 0..99 {
            let mut w = spawn(b.wait());
            assert_pending!(w.poll());
            wait.push(w);
        }
        for w in &mut wait {
            assert_pending!(w.poll());
        }

        // pass the barrier
        let mut w = spawn(b.wait());
        let mut found_leader = assert_ready!(w.poll());
        for mut w in wait {
            let wr = assert_ready!(w.poll());
            if wr {
                assert!(!found_leader);
                found_leader = true;
            }
        }
        assert!(found_leader);
    }
}
