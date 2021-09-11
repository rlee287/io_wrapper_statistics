use io_wrapper_statistics::{IOStatWrapper, IopInfoPair, SuccessFailureCounter};

use std::io::{Read, Seek, Write, SeekFrom, Cursor};

#[test]
fn test_basic_counts() {
    let mut init_data_buf = [0, 1, 2, 3, 4, 5, 6, 7];
    let base_io_obj: Cursor<&mut [u8]> = Cursor::new(&mut init_data_buf[..]);
    let mut io_wrapper = IOStatWrapper::<_,Vec<IopInfoPair>>::new(base_io_obj, 0);
    let mut slice_buf: [u8; 8] = [0; 8];

    io_wrapper.read(&mut slice_buf).unwrap();
    io_wrapper.seek(SeekFrom::Start(4)).unwrap();
    io_wrapper.write(&mut slice_buf[..4]).unwrap();

    let mut io_count_expect = SuccessFailureCounter::<u64>::default();
    io_count_expect.increment_success();

    println!("{:#?}", io_wrapper);

    assert_eq!(io_wrapper.read_call_counter(), &io_count_expect);
    assert_eq!(io_wrapper.read_byte_counter(), 8);
    assert_eq!(io_wrapper.seek_call_counter(), &io_count_expect);
    assert_eq!(io_wrapper.seek_pos(), io_wrapper.stream_position().unwrap());
    assert_eq!(io_wrapper.write_call_counter(), &io_count_expect);
    assert_eq!(io_wrapper.write_byte_counter(), 4);
}