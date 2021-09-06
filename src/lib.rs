#![forbid(unsafe_code)]

use std::io::{Read, Write, Seek, SeekFrom};
use std::io::Result as IOResult;
use std::io::ErrorKind;
use std::convert::TryFrom;

use std::iter::Extend;

use num_traits::{PrimInt, Unsigned, Signed};

pub use success_failure_ctr::SuccessFailureCounter;

pub mod success_failure_ctr {
    use num_traits::{PrimInt, Unsigned};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct SuccessFailureCounter<T: PrimInt + Unsigned> {
        success_ctr: T,
        failure_ctr: T
    }
    impl<T: PrimInt + Unsigned> SuccessFailureCounter<T> {
        pub fn increment_success(&mut self) {
            self.success_ctr = self.success_ctr + T::one();
        }
        pub fn success_ctr(&self) -> T {
            self.success_ctr
        }
        pub fn increment_failure(&mut self) {
            self.failure_ctr = self.failure_ctr + T::one();
        }
        pub fn failure_ctr(&self) -> T {
            self.failure_ctr
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SignedAbsResult<T: PrimInt + Unsigned> {
    Negative(T),
    Zero,
    Positive(T)
}
fn abs_sign_tuple<S, U>(signed_number: S) -> SignedAbsResult<U>
where
    S: PrimInt + Signed,
    U: PrimInt + Unsigned
{
    if signed_number.signum() == S::one() {
        SignedAbsResult::Positive(U::from(signed_number.abs()).unwrap())
    } else if signed_number.signum() == S::zero() {
        SignedAbsResult::Zero
    } else if signed_number.signum() == -S::one() {
        if signed_number == S::min_value() {
            // .abs would be borked-do manually
            // Primitive integer types guaranteed to be two's complement
            SignedAbsResult::Negative(U::from(S::max_value()).unwrap()+U::one())
        } else {
            SignedAbsResult::Negative(U::from(signed_number.abs()).unwrap())
        }
    } else {
        unreachable!()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum IopActions {
    Read(usize),
    Seek(SeekFrom),
    Write(usize),
    Flush
}
#[derive(Debug, Clone, Copy)]
pub enum IopResults {
    Read(Result<usize, ErrorKind>),
    Seek(Result<u64, ErrorKind>),
    Write(Result<usize, ErrorKind>),
    Flush(Result<(), ErrorKind>)
}
type IopInfoPair = (IopActions, IopResults);

#[derive(Debug)]
pub struct IOStatWrapper<T, C> {
    inner_io: T,
    iop_log: C,
    read_call_counter: SuccessFailureCounter<u64>,
    read_byte_counter: usize,
    seek_call_counter: SuccessFailureCounter<u64>,
    seek_pos: u64, // Meaningless unless T: Seek
    write_call_counter: SuccessFailureCounter<u64>,
    write_flush_counter: u64,
    write_byte_counter: usize
}

impl<T, C> IOStatWrapper<T, C>
where
    C: Default + Extend<IopInfoPair>
{
    /// Object must be passed in immediately
    pub fn new(obj: T, start_seek_pos: u64) -> IOStatWrapper<T, C> {
        IOStatWrapper {
            inner_io: obj,
            iop_log: C::default(),
            read_call_counter: SuccessFailureCounter::default(),
            read_byte_counter: 0,
            seek_call_counter: SuccessFailureCounter::default(),
            seek_pos: start_seek_pos,
            write_call_counter: SuccessFailureCounter::default(),
            write_flush_counter: 0,
            write_byte_counter: 0
        }
    }
    pub fn into_inner(self) -> T {
        self.inner_io
    }
    pub fn iop_log(&self) -> &C {
        &self.iop_log
    }
}

impl<T: Read, C: Extend<IopInfoPair>> Read for IOStatWrapper<T, C> {
    fn read(&mut self, buf: &mut [u8]) -> IOResult<usize> {
        let read_result = self.inner_io.read(buf);
        let extend_item: [IopInfoPair; 1] = match read_result {
            Ok(n) => {
                self.read_call_counter.increment_success();
                self.read_byte_counter += n;
                self.seek_pos += u64::try_from(n).unwrap();
                [(IopActions::Read(buf.len()),
                    IopResults::Read(Ok(n)))]
            },
            Err(ref e) => {
                self.read_call_counter.increment_failure();
                [(IopActions::Read(buf.len()),
                    IopResults::Read(Err(e.kind())))]
            }
        };
        self.iop_log.extend(extend_item);
        read_result
    }
}
impl<T: Read, C> IOStatWrapper<T, C> {
    pub fn read_call_counter(&self) -> &SuccessFailureCounter<u64> {
        &self.read_call_counter
    }
    pub fn read_byte_counter(&self) -> usize {
        self.read_byte_counter
    }
}

impl<T: Seek, C: Extend<IopInfoPair>> Seek for IOStatWrapper<T, C> {
    fn seek(&mut self, pos: SeekFrom) -> IOResult<u64> {
        let old_pos = self.seek_pos;
        let seek_result = self.inner_io.seek(pos);
        let extend_item: [IopInfoPair; 1] = match seek_result {
            Ok(n) => {
                self.seek_call_counter.increment_success();
                self.seek_pos = n;
                if let SeekFrom::Current(offset) = pos {
                    match abs_sign_tuple::<i64, u64>(offset) {
                        SignedAbsResult::Zero => {
                            debug_assert_eq!(old_pos, n);
                        },
                        SignedAbsResult::Positive(a) => {
                            debug_assert_eq!(old_pos+a, n)
                        },
                        SignedAbsResult::Negative(a) => {
                            debug_assert_eq!(old_pos-a, n)
                        }
                    }
                };
                [(IopActions::Seek(pos),
                    IopResults::Seek(Ok(n)))]
            },
            Err(ref e) => {
                self.seek_call_counter.increment_failure();
                [(IopActions::Seek(pos),
                    IopResults::Seek(Err(e.kind())))]
            }
        };
        self.iop_log.extend(extend_item);
        seek_result
    }
}
impl<T: Seek, C> IOStatWrapper<T, C> {
    pub fn seek_call_counter(&self) -> &SuccessFailureCounter<u64> {
        &self.seek_call_counter
    }
    pub fn seek_pos(&self) -> u64 {
        self.seek_pos
    }
}

impl<T: Write, C: Extend<IopInfoPair>> Write for IOStatWrapper<T, C> {
    fn write(&mut self, buf: &[u8]) -> IOResult<usize> {
        let write_result = self.inner_io.write(buf);
        let extend_item: [IopInfoPair; 1] = match write_result {
            Ok(n) => {
                self.write_call_counter.increment_success();
                self.write_byte_counter += n;
                self.seek_pos += u64::try_from(n).unwrap();
                [(IopActions::Write(buf.len()),
                    IopResults::Write(Ok(n)))]
            },
            Err(ref e) => {
                self.write_call_counter.increment_failure();
                [(IopActions::Write(buf.len()),
                    IopResults::Write(Err(e.kind())))]
            }
        };
        self.iop_log.extend(extend_item);
        write_result
    }
    fn flush(&mut self) -> IOResult<()> {
        self.write_flush_counter += 1;
        let flush_result = self.inner_io.flush();
        let extend_item: [IopInfoPair; 1] = match flush_result {
            Ok(()) => [(IopActions::Flush, IopResults::Flush(Ok(())))],
            Err(ref e) => 
                [(IopActions::Flush,
                    IopResults::Flush(Err(e.kind())))]
        };
        self.iop_log.extend(extend_item);
        flush_result
    }
}
impl<T: Write, C> IOStatWrapper<T, C> {
    pub fn write_call_counter(&self) -> &SuccessFailureCounter<u64> {
        &self.write_call_counter
    }
    pub fn write_flush_counter(&self) -> u64 {
        self.write_flush_counter
    }
    pub fn write_byte_counter(&self) -> usize {
        self.write_byte_counter
    }
}
