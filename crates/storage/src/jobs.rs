// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{Database, Error, MAX_RECEIPT_BYTES, Receipt, Result, valid_key};
use rand_core::{OsRng, RngCore};
use rusqlite::{OptionalExtension, Transaction, params, params_from_iter, types::Value};
use std::time::Duration;

pub(crate) const SCHEMA:&str="CREATE TABLE core_jobs(
 id INTEGER PRIMARY KEY AUTOINCREMENT, queue TEXT NOT NULL, job_key TEXT NOT NULL, kind TEXT NOT NULL,
 payload BLOB NOT NULL, initial_ready_at INTEGER, ready_at INTEGER,
 state TEXT NOT NULL CHECK(state IN('ready','running','paused','done')), attempts INTEGER NOT NULL DEFAULT 0,
 claim_token BLOB, lease_until INTEGER, response BLOB, error_code TEXT, retry_condition TEXT,
 UNIQUE(queue,job_key));
 CREATE INDEX core_jobs_ready ON core_jobs(queue,state,ready_at,id);";

pub struct JobSpec<'a> {
    pub queue: &'a str,
    pub key: &'a str,
    pub kind: &'a str,
    pub payload: &'a [u8],
    pub ready_at: Option<i64>,
}
#[derive(Clone)]
pub struct Claim {
    pub id: i64,
    pub attempt: u64,
    pub queue: String,
    pub kind: String,
    pub payload: Vec<u8>,
    token: Vec<u8>,
}
impl std::fmt::Debug for Claim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Claim")
            .field("id", &self.id)
            .field("attempt", &self.attempt)
            .field("queue", &self.queue)
            .field("kind", &self.kind)
            .field("payload_bytes", &self.payload.len())
            .finish_non_exhaustive()
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct Status {
    pub state: String,
    pub attempts: u64,
    pub error_code: Option<String>,
    pub retry_condition: Option<String>,
}
pub enum Retry {
    At(i64),
    Manual,
}
type ActiveRow = (String, Vec<u8>, Option<i64>, Option<Vec<u8>>);

/// Transactional outbox insertion; uses the caller's existing command/finish transaction.
pub fn enqueue(tx: &Transaction<'_>, job: JobSpec<'_>) -> Result<i64> {
    if !valid_key(job.queue)
        || !valid_key(job.key)
        || !valid_key(job.kind)
        || job.payload.len() > MAX_RECEIPT_BYTES
    {
        return Err(Error::InvalidJob);
    }
    let existing: Option<(i64, String, Vec<u8>, Option<i64>)> = tx
        .query_row(
            "SELECT id,kind,payload,initial_ready_at FROM core_jobs WHERE queue=?1 AND job_key=?2",
            [job.queue, job.key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    if let Some((id, kind, payload, ready_at)) = existing {
        if kind != job.kind || payload != job.payload || ready_at != job.ready_at {
            return Err(Error::ReusedJobId);
        }
        return Ok(id);
    }
    tx.execute("INSERT INTO core_jobs(queue,job_key,kind,payload,initial_ready_at,ready_at,state) VALUES(?1,?2,?3,?4,?5,?5,'ready')",params![job.queue,job.key,job.kind,job.payload,job.ready_at])?;
    Ok(tx.last_insert_rowid())
}
fn lease_end(now: i64, lease: Duration) -> Result<i64> {
    let duration = i64::try_from(lease.as_millis()).map_err(|_| Error::InvalidConfiguration)?;
    if now < 0 || duration <= 0 {
        return Err(Error::InvalidConfiguration);
    }
    now.checked_add(duration).ok_or(Error::InvalidConfiguration)
}
fn active(tx: &Transaction<'_>, claim: &Claim, now: i64) -> Result<(String, Option<Vec<u8>>)> {
    let row:Option<ActiveRow>=tx.query_row("SELECT state,claim_token,lease_until,response FROM core_jobs WHERE id=?1 AND attempts=?2",params![claim.id,i64::try_from(claim.attempt).map_err(|_|Error::StaleClaim)?],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let Some((state, token, until, response)) = row else {
        return Err(Error::StaleClaim);
    };
    if token != claim.token
        || (state != "done" && (state != "running" || until.is_none_or(|end| end <= now)))
    {
        return Err(Error::StaleClaim);
    }
    Ok((state, response))
}
impl Database {
    pub fn enqueue(&mut self, job: JobSpec<'_>) -> Result<i64> {
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let id = enqueue(&tx, job)?;
        tx.commit()?;
        Ok(id)
    }
    pub fn claim_job(
        &mut self,
        queue: &str,
        kinds: &[&str],
        lease: Duration,
    ) -> Result<Option<Claim>> {
        if !valid_key(queue) || kinds.len() > 64 || kinds.iter().any(|kind| !valid_key(kind)) {
            return Err(Error::InvalidJob);
        }
        if kinds.is_empty() {
            return Ok(None);
        }
        let now = self.clock.monotonic_millis();
        let until = lease_end(now, lease)?;
        let mut arguments = vec![
            Value::Text(queue.into()),
            Value::Integer(self.clock.wall_millis()),
            Value::Integer(now),
        ];
        arguments.extend(kinds.iter().map(|kind| Value::Text((*kind).into())));
        let placeholders = (4..4 + kinds.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id,kind,payload,attempts FROM core_jobs WHERE queue=?1 AND ((state='ready' AND (ready_at IS NULL OR ready_at<=?2)) OR (state='running' AND lease_until<=?3)) AND kind IN({placeholders}) ORDER BY id LIMIT 1"
        );
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let row: Option<(i64, String, Vec<u8>, i64)> = tx
            .query_row(&sql, params_from_iter(arguments), |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .optional()?;
        let Some((id, kind, payload, attempts)) = row else {
            tx.commit()?;
            return Ok(None);
        };
        let attempts = attempts
            .checked_add(1)
            .filter(|attempt| *attempt > 0)
            .ok_or(Error::InvalidJob)?;
        let mut token = [0u8; 16];
        OsRng.fill_bytes(&mut token);
        tx.execute("UPDATE core_jobs SET state='running',attempts=?1,claim_token=?2,lease_until=?3 WHERE id=?4",params![attempts,token.as_slice(),until,id])?;
        tx.commit()?;
        Ok(Some(Claim {
            id,
            attempt: u64::try_from(attempts).map_err(|_| Error::InvalidJob)?,
            queue: queue.into(),
            kind,
            payload,
            token: token.to_vec(),
        }))
    }
    pub fn finish_job(
        &mut self,
        claim: &Claim,
        work: impl FnOnce(&Transaction<'_>) -> Result<Vec<u8>>,
    ) -> Result<Receipt> {
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (state, response) = active(&tx, claim, self.clock.monotonic_millis())?;
        if state == "done" {
            tx.commit()?;
            return Ok(Receipt {
                bytes: response.ok_or(Error::InvalidJob)?,
                replayed: true,
            });
        }
        let bytes = work(&tx)?;
        if bytes.len() > MAX_RECEIPT_BYTES {
            return Err(Error::ReceiptTooLarge);
        }
        active(&tx, claim, self.clock.monotonic_millis())?;
        tx.execute("UPDATE core_jobs SET state='done',response=?1,lease_until=NULL,error_code=NULL,retry_condition=NULL WHERE id=?2",params![bytes,claim.id])?;
        tx.commit()?;
        Ok(Receipt {
            bytes,
            replayed: false,
        })
    }
    pub fn renew_claim(&mut self, claim: &Claim, lease: Duration) -> Result<()> {
        let now = self.clock.monotonic_millis();
        let until = lease_end(now, lease)?;
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if active(&tx, claim, now)?.0 != "running" {
            return Err(Error::StaleClaim);
        }
        tx.execute(
            "UPDATE core_jobs SET lease_until=?1 WHERE id=?2",
            params![until, claim.id],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn retry_job(&mut self, claim: &Claim, code: &str, retry: Retry) -> Result<()> {
        if !valid_key(code) {
            return Err(Error::InvalidJob);
        }
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if active(&tx, claim, self.clock.monotonic_millis())?.0 != "running" {
            return Err(Error::StaleClaim);
        }
        let (state, at, condition) = match retry {
            Retry::At(at) => ("ready", Some(at), "local_time"),
            Retry::Manual => ("paused", None, "operator_or_dependency"),
        };
        tx.execute("UPDATE core_jobs SET state=?1,ready_at=?2,lease_until=NULL,error_code=?3,retry_condition=?4 WHERE id=?5",params![state,at,code,condition,claim.id])?;
        tx.commit()?;
        Ok(())
    }
    pub fn resume_job(&mut self, id: i64, ready_at: Option<i64>) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE core_jobs SET state='ready',ready_at=?1 WHERE id=?2 AND state='paused'",
            params![ready_at, id],
        )? == 1)
    }
    pub fn job_status(&self, id: i64) -> Result<Option<Status>> {
        Ok(self
            .connection
            .query_row(
                "SELECT state,attempts,error_code,retry_condition FROM core_jobs WHERE id=?1",
                [id],
                |r| {
                    Ok(Status {
                        state: r.get(0)?,
                        attempts: r.get::<_, i64>(1)?.try_into().map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                1,
                                rusqlite::types::Type::Integer,
                                Box::new(error),
                            )
                        })?,
                        error_code: r.get(2)?,
                        retry_condition: r.get(3)?,
                    })
                },
            )
            .optional()?)
    }
    pub(crate) fn recover_running_jobs(&mut self) -> Result<usize> {
        Ok(self.connection.execute("UPDATE core_jobs SET state='ready',ready_at=NULL,lease_until=NULL,error_code='writer_restarted',retry_condition='restart' WHERE state='running'",[])?)
    }
}
