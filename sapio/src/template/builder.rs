use super::Template;
pub use super::{Output, OutputMeta};
use crate::contract::{CompilationError, Context};
use bitcoin::util::amount::Amount;
use sapio_base::timelocks::*;
use sapio_base::CTVHash;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::convert::TryInto;

/// Builder can be used to interactively put together a transaction template before
/// finalizing into a Template.
pub struct Builder {
    sequences: Vec<Option<AnyRelTimeLock>>,
    outputs: Vec<Output>,
    version: i32,
    lock_time: Option<AnyAbsTimeLock>,
    label: String,
    ctx: Context,
}

impl Builder {
    /// Creates a new transaction template with 1 input and no outputs.
    pub fn new(ctx: Context) -> Builder {
        Builder {
            sequences: vec![None],
            outputs: vec![],
            version: 2,
            lock_time: None,
            label: String::new(),
            ctx,
        }
    }
    pub fn spend_amount(mut self, amount: Amount) -> Result<Self, CompilationError> {
        self.ctx.spend_amount(amount)?;
        Ok(self)
    }

    /// Creates a new Output, forcing the compilation of the compilable object and defaulting
    /// metadata if not provided to blank.
    pub fn add_output<T: crate::contract::Compilable>(
        mut self,
        amount: Amount,
        contract: &T,
        metadata: Option<OutputMeta>,
    ) -> Result<Self, CompilationError> {
        self.outputs.push(Output {
            amount: amount,
            contract: contract.compile(&self.ctx.with_amount(amount)?)?,
            metadata: metadata.unwrap_or_else(HashMap::new),
        });
        self.spend_amount(amount)
    }

    // TODO: Make guarantee there is some external input?
    pub fn add_amount(mut self, a: Amount) -> Self {
        self.ctx.add_amount(a);
        self
    }

    /// Adds another output. Follow with a call to
    /// set_sequence(-1, ...) to fill in the back.
    pub fn add_sequence(mut self) -> Self {
        self.sequences.push(None);
        self
    }
    /// set_sequence adds a height or time based relative lock time to the
    /// template. If a lock time is already set, it will check if it is of the
    /// same kind. Differing kinds will throw an error. Otherwise, it will merge
    /// by taking the max of the argument.
    ///
    /// Negative indexing allows us to work from the back element easily
    pub fn set_sequence(mut self, ii: isize, s: AnyRelTimeLock) -> Result<Self, CompilationError> {
        let i = if ii >= 0 {
            ii
        } else {
            self.sequences.len() as isize + ii
        } as usize;
        match self.sequences.get_mut(i).as_mut() {
            Some(Some(seq)) => match (*seq, s) {
                (a @ AnyRelTimeLock::RH(_), b @ AnyRelTimeLock::RH(_)) => {
                    *seq = std::cmp::max(a, b);
                }
                (a @ AnyRelTimeLock::RT(_), b @ AnyRelTimeLock::RT(_)) => {
                    *seq = std::cmp::max(a, b);
                }
                _ => return Err(CompilationError::IncompatibleSequence),
            },
            Some(x @ None) => {
                x.replace(s);
            }
            None => return Err(CompilationError::NoSuchSequence),
        };
        Ok(self)
    }
    /// set_lock_time adds a height or time based absolute lock time to the
    /// template. If a lock time is already set, it will check if it is of the
    /// same kind. Differing kinds will throw an error. Otherwise, it will merge
    /// by taking the max of the argument.
    pub fn set_lock_time(mut self, lt_in: AnyAbsTimeLock) -> Result<Self, CompilationError> {
        if let Some(lt) = self.lock_time.as_mut() {
            match (*lt, lt_in) {
                (a @ AnyAbsTimeLock::AH(_), b @ AnyAbsTimeLock::AH(_)) => {
                    *lt = std::cmp::max(a, b);
                }
                (a @ AnyAbsTimeLock::AT(_), b @ AnyAbsTimeLock::AT(_)) => {
                    *lt = std::cmp::max(a, b);
                }
                _ => return Err(CompilationError::IncompatibleSequence),
            }
        } else {
            self.lock_time = Some(lt_in);
        }
        Ok(self)
    }

    pub fn set_label(mut self, label: String) -> Self {
        self.label = label;
        self
    }

    /// Creates a transaction from a Builder.
    /// Generally, should not be called directly.
    pub fn get_tx(&self) -> bitcoin::Transaction {
        let default_seq = RelTime::try_from(0).unwrap().into();
        let default_nlt = AbsHeight::try_from(0).unwrap().into();
        bitcoin::Transaction {
            version: self.version,
            lock_time: self.lock_time.unwrap_or(default_nlt).get(),
            input: self
                .sequences
                .iter()
                .map(|sequence| bitcoin::TxIn {
                    previous_output: Default::default(),
                    script_sig: Default::default(),
                    sequence: sequence.unwrap_or(default_seq).get(),
                    witness: vec![],
                })
                .collect(),
            output: self
                .outputs
                .iter()
                .map(|out| bitcoin::TxOut {
                    value: TryInto::<Amount>::try_into(out.amount).unwrap().as_sat(),
                    script_pubkey: out.contract.address.script_pubkey(),
                })
                .collect(),
        }
    }
}
impl From<Builder> for Template {
    fn from(t: Builder) -> Template {
        let tx = t.get_tx();
        Template {
            outputs: t.outputs,
            ctv: tx.get_ctv_hash(0),
            max: tx.total_amount().into(),
            tx,
            label: t.label,
        }
    }
}
/// We don't implement TryFrom because this can never actually fail!
/// We want to be able to use this anywhere we use into
impl From<Builder> for Result<Template, CompilationError> {
    fn from(t: Builder) -> Self {
        Ok(t.into())
    }
}

impl From<Builder> for crate::contract::TxTmplIt {
    fn from(t: Builder) -> Self {
        // t.into() // works too, but prefer the explicit form so we know what we get concretely
        Ok(Box::new(std::iter::once(Result::<
            Template,
            CompilationError,
        >::from(t))))
    }
}