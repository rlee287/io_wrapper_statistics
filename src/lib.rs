#![forbid(unsafe_code)]

use std::io::{Read, Write, Seek, SeekFrom};
use std::io::Result as IOResult;
use std::io::ErrorKind;
use std::io::{IoSlice, IoSliceMut};

use std::convert::TryFrom;

use std::iter::Extend;

use num_traits::{PrimInt, Unsigned, Signed};

pub use success_failure_ctr::SuccessFailureCounter;

pub mod success_failure_ctr {
    use num_traits::{PrimInt, Unsigned};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    /// A struct for counting successful and failed attempts.
    pub struct SuccessFailureCounter<T: PrimInt + Unsigned> {
        success_ctr: T,
        failure_ctr: T
    }
    impl<T: PrimInt + Unsigned> SuccessFailureCounter<T> {
        pub fn increment_success(&mut self) {
            self.success_ctr = self.success_ctr + T::one();
        }
        pub fn add_successes(&mut self, amount: T) {
            self.success_ctr = self.success_ctr + amount;
        }
        pub fn success_ctr(&self) -> T {
            self.success_ctr
        }
        pub fn increment_failure(&mut self) {
            self.failure_ctr = self.failure_ctr + T::one();
        }
        pub fn add_failures(&mut self, amount: T) {
            self.failure_ctr = self.failure_ctr + amount;
        }
        pub fn failure_ctr(&self) -> T {
            self.failure_ctr
        }
        pub fn attempt_ctr(&self) -> T {
            self.success_ctr + self.failure_ctr
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SignedAbsResult<T: PrimInt + Unsigned> {
    Negative(T),
    Zero,
    Positive(T)
}
/// Returns the absolute value of a signed number, along with the original sign.
/// Needed because abs(i*::MIN) returns a signed value and is still negative
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
/// Types of IO Operations.
pub enum IopActions {
    /// Attempted read of the given size.
    Read(usize),
    /// Attempted seek to the given position.
    Seek(SeekFrom),
    /// Attempted write of the given size.
    Write(usize),
    /// Attempted flush of a writer.
    Flush
}
#[derive(Debug, Clone, Copy)]
/// Results of IO Operations.
///
/// We store only ErrorKind because IOError is not Clonable and Arc<&IOError> would be messy with lifetimes.
pub enum IopResults {
    /// Result of a read operation.
    Read(Result<usize, ErrorKind>),
    /// Result of a seek operation.
    Seek(Result<u64, ErrorKind>),
    /// Result of a write operation.
    Write(Result<usize, ErrorKind>),
    /// Result of a flush operation.
    Flush(Result<(), ErrorKind>)
}
pub type IopInfoPair = (IopActions, IopResults);

#[derive(Debug)]
/// A wrapper around an IO object that tracks operations and statistics.
pub struct IOStatWrapper<T, C> {
    inner_io: T,
    iop_log: C,
    read_call_counter: SuccessFailureCounter<u64>,
    read_byte_counter: usize,
    seek_call_counter: SuccessFailureCounter<u64>,
    seek_pos: u64, // Meaningless unless T: Seek
    write_call_counter: SuccessFailureCounter<u64>,
    write_flush_counter: SuccessFailureCounter<u64>,
    write_byte_counter: usize
}

impl<T, C> IOStatWrapper<T, C>
where
    C: Default + Extend<IopInfoPair>
{
    /// Create a new IOStatWrapper with a manually given seek position.
    /// Detecting the seek position automatically is not possible without specialization.
    pub fn new(obj: T, start_seek_pos: u64) -> IOStatWrapper<T, C> {
        IOStatWrapper {
            inner_io: obj,
            iop_log: C::default(),
            read_call_counter: SuccessFailureCounter::default(),
            read_byte_counter: 0,
            seek_call_counter: SuccessFailureCounter::default(),
            seek_pos: start_seek_pos,
            write_call_counter: SuccessFailureCounter::default(),
            write_flush_counter: SuccessFailureCounter::default(),
            write_byte_counter: 0
        }
    }
    /// Extract the original IO object.
    pub fn into_inner(self) -> T {
        self.inner_io
    }
    /// Get the IO operation log containing operations and their results.
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

    #[rustversion::since(1.36)]
    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> IOResult<usize> {
        self.inner_io.read_vectored(bufs)
    }
    #[rustversion::nightly]
    fn is_read_vectored(&self) -> bool {
        self.inner_io.is_read_vectored()
    }
    #[rustversion::nightly]
    #[inline]
    unsafe fn initializer(&self) -> Initializer {
        self.inner_io.initializer()
    }
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> IOResult<usize> {
        self.inner_io.read_to_end(buf)
    }
    fn read_to_string(&mut self, buf: &mut String) -> IOResult<usize> {
        self.inner_io.read_to_string(buf)
    }
    #[rustversion::since(1.6)]
    fn read_exact(&mut self, buf: &mut [u8]) -> IOResult<()> {
        self.inner_io.read_exact(buf)
    }
    fn by_ref(&mut self) -> &mut Self
    where
        Self: Sized,
    {
        // Do not pass this one through to the inner_io object
        self
    }
    // Missing: bytes, chain, and take, as the struct fields are private
    // Issues arise if default impls are overriden, but this is unlikely

    /*fn bytes(self) -> Bytes<Self>
    where
        Self: Sized,
    {
        Bytes{inner: self}
    }
    fn chain<R: Read>(self, next: R) -> Chain<Self, R>
    where
        Self: Sized,
    {
        Chain{first: self, second: next, done_first: false}
    }
    fn take(self, limit: u64) -> Take<Self>
    where
        Self: Sized,
    {
        Take{inner: self, limit}
    }*/
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
    /*
     * For provided methods, do not do logging in them
     * If inner's impl calls seek then it gets logged already
     * If inner's impl avoids seek (e.g. stream_position) then no operation occured
     */
    #[rustversion::since(1.55)]
    fn rewind(&mut self) -> IOResult<()> {
        self.inner_io.rewind()
    }
    #[rustversion::nightly]
    fn stream_len(&mut self) -> IOResult<u64> {
        self.inner_io.stream_len()
    }
    #[rustversion::since(1.51)]
    fn stream_position(&mut self) -> IOResult<u64> {
        self.inner_io.stream_position()
    }
}
impl<T: Seek, C> IOStatWrapper<T, C> {
    pub fn seek_call_counter(&self) -> &SuccessFailureCounter<u64> {
        &self.seek_call_counter
    }
    /// Get the current seek position without doing an actual seek operation.
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
        let flush_result = self.inner_io.flush();
        let extend_item: [IopInfoPair; 1] = match flush_result {
            Ok(()) => {
                self.write_flush_counter.increment_success();
                [(IopActions::Flush, IopResults::Flush(Ok(())))]
            },
            Err(ref e) => {
                self.write_flush_counter.increment_failure();
                [(IopActions::Flush,
                    IopResults::Flush(Err(e.kind())))]
            }
        };
        self.iop_log.extend(extend_item);
        flush_result
    }

    #[rustversion::since(1.36.0)]
    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> IOResult<usize> {
        self.inner_io.write_vectored(bufs)
    }
    #[rustversion::nightly]
    fn is_write_vectored(&self) -> bool {
        self.inner_io.is_write_vectored()
    }
    fn write_all(&mut self, mut buf: &[u8]) -> IOResult<()> {
        self.inner_io.write_all(buf)
    }
    #[rustversion::nightly]
    fn write_all_vectored(&mut self, mut bufs: &mut [IoSlice<'_>]) -> IOResult<()> {
        self.inner_io.write_all_vectored(bufs)
    }
    fn write_fmt(&mut self, fmt: std::fmt::Arguments<'_>) -> IOResult<()> {
        self.inner_io.write_fmt(fmt)
    }
    fn by_ref(&mut self) -> &mut Self
    where
        Self: Sized,
    {
        // Do not pass this one through to the inner_io object
        self
    }
}
impl<T: Write, C> IOStatWrapper<T, C> {
    pub fn write_call_counter(&self) -> &SuccessFailureCounter<u64> {
        &self.write_call_counter
    }
    pub fn write_flush_counter(&self) -> &SuccessFailureCounter<u64> {
        &self.write_flush_counter
    }
    pub fn write_byte_counter(&self) -> usize {
        self.write_byte_counter
    }
}
