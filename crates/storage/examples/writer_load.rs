// SPDX-License-Identifier: AGPL-3.0-or-later
//! Finite real-SQLite load observation, not a minimum hardware or capacity promise.
use bbsbox_storage::{
    Command, Migration,
    jobs::{JobSpec, enqueue},
    open_reader,
    writer::{Config, Priority, Writer},
};
use std::{
    collections::VecDeque,
    thread,
    time::{Duration, Instant},
};

fn main() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("load.sqlite");
    let writer=Writer::start(&path,vec![Migration{module:"measure",version:1,sql:"CREATE TABLE measure_items(id INTEGER PRIMARY KEY); CREATE TABLE measure_clicks(id INTEGER PRIMARY KEY);"}],Config::default()).unwrap();
    let jobs = 200;
    let requests = 400i64;
    writer
        .handle()
        .submit(Priority::Interactive, move |db| {
            db.transaction(
                Command {
                    namespace: "measure/setup",
                    id: "jobs",
                    request: b"200-jobs.v1",
                },
                |tx| {
                    for batch in 0i64..jobs {
                        enqueue(
                            tx,
                            JobSpec {
                                queue: "measure",
                                key: &format!("batch-{batch}"),
                                kind: "import.v1",
                                payload: &batch.to_le_bytes(),
                                ready_at: None,
                            },
                        )?;
                    }
                    Ok(vec![])
                },
            )
        })
        .unwrap()
        .blocking_wait()
        .unwrap();
    let start = Instant::now();
    let interactive = writer.handle();
    let background = writer.handle();
    let local = thread::spawn(move || {
        let mut pending = VecDeque::new();
        let mut timings = Vec::new();
        for index in 0..requests {
            let started = Instant::now();
            pending.push_back(
                interactive
                    .submit(Priority::Interactive, move |db| {
                        db.transaction(
                            Command {
                                namespace: "measure/local",
                                id: &format!("request-{index}"),
                                request: &index.to_le_bytes(),
                            },
                            |tx| {
                                tx.execute("INSERT INTO measure_clicks VALUES(?1)", [index])?;
                                Ok(vec![])
                            },
                        )?;
                        Ok(started.elapsed().as_micros())
                    })
                    .unwrap(),
            );
            if pending.len() >= 32 {
                timings.push(pending.pop_front().unwrap().blocking_wait().unwrap());
            }
        }
        for request in pending {
            timings.push(request.blocking_wait().unwrap());
        }
        timings
    });
    let imports = thread::spawn(move || {
        let mut done = 0;
        loop {
            let claim = background
                .submit(Priority::Background, |db| {
                    db.claim_job("measure", &["import.v1"], Duration::from_secs(60))
                })
                .unwrap()
                .blocking_wait()
                .unwrap();
            let Some(claim) = claim else {
                break;
            };
            background
                .submit(Priority::Background, move |db| {
                    db.finish_job(&claim, |tx| {
                        let batch =
                            i64::from_le_bytes(claim.payload.as_slice().try_into().unwrap());
                        let mut insert =
                            tx.prepare_cached("INSERT INTO measure_items VALUES(?1)")?;
                        for row in 0..100 {
                            insert.execute([batch * 100 + row])?;
                        }
                        Ok(vec![])
                    })
                })
                .unwrap()
                .blocking_wait()
                .unwrap();
            done += 1;
        }
        done
    });
    let mut timings = local.join().unwrap();
    let done = imports.join().unwrap();
    let elapsed = start.elapsed().as_millis();
    writer.shutdown().unwrap();
    let reader = open_reader(&path).unwrap();
    let items: i64 = reader
        .query_row("SELECT count(*) FROM measure_items", [], |r| r.get(0))
        .unwrap();
    let clicks: i64 = reader
        .query_row("SELECT count(*) FROM measure_clicks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(done, jobs);
    assert_eq!(items, jobs * 100);
    assert_eq!(clicks, requests);
    timings.sort_unstable();
    println!(
        "{{\"interactive_requests\":{clicks},\"background_jobs\":{done},\"import_rows\":{items},\"batch_rows\":100,\"pending_capacity\":64,\"interactive_burst\":8,\"elapsed_ms\":{elapsed},\"local_p50_us\":{},\"local_p95_us\":{},\"local_max_us\":{}}}",
        timings[timings.len() / 2],
        timings[timings.len() * 95 / 100],
        timings.last().unwrap()
    );
}
