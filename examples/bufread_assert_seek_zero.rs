use io_wrapper_statistics::{IOStatWrapper, IopInfoPair};

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, BufReader};

fn main() {
    let file_obj = File::open("Cargo.toml").unwrap();
    let mut instrumented_raw_file = IOStatWrapper::<_, Vec<IopInfoPair>>::new(file_obj, 0);
    let buffered_io = BufReader::new(&mut instrumented_raw_file);
    let mut instrumented_buf_file = IOStatWrapper::<_, Vec<IopInfoPair>>::new(buffered_io, 0);

    // Do something so that the loop doesn't get optimized out
    let mut xor_result: u8 = 0x00;
    let mut read_buf: [u8; 1] = [0x00; 1];
    loop {
        let bytes_read = instrumented_buf_file.read(&mut read_buf).unwrap();
        if bytes_read == 0 {
            break;
        }
        xor_result ^= read_buf[0];
        assert_eq!(instrumented_buf_file.seek_pos(),
            instrumented_buf_file.seek(SeekFrom::Current(0)).unwrap());
    }
    println!("XOR of all bytes in Cargo.toml is {:#x}", xor_result);

    // Demonstrate how BufReader reduces the number of read calls
    println!("Buffered read was called successfully {} times",
        instrumented_buf_file.read_call_counter().success_ctr());
    println!("Buffered seek was called successfully {} times",
        instrumented_buf_file.seek_call_counter().success_ctr());
    println!("Inner read was called successfully {} times",
        instrumented_raw_file.read_call_counter().success_ctr());
    println!("Inner seek was called successfully {} times",
        instrumented_raw_file.seek_call_counter().success_ctr());
}