use rurge_client::sync::triple_buffer::triple_buffer;
use std::thread;

#[test]
fn test_triple_buffer_basic_exchange() {
    let (mut producer, mut consumer) = triple_buffer(|| 0u64);

    producer.write(|val| *val = 42);
    assert_eq!(consumer.read(), Some(&42));
    assert_eq!(consumer.read(), None); // No new update
}

#[test]
fn test_triple_buffer_thread_stress() {
    let (mut producer, mut consumer) = triple_buffer(|| Vec::<u32>::with_capacity(100));

    let p_handle = thread::spawn(move || {
        for i in 0..10_000 {
            producer.write(|v| {
                v.clear();
                v.push(i);
            });
        }
    });

    let c_handle = thread::spawn(move || {
        let mut last = 0;
        let mut reads = 0;
        while last < 9_999 {
            if let Some(v) = consumer.read() {
                if let Some(&val) = v.first() {
                    assert!(val >= last);
                    last = val;
                    reads += 1;
                }
            }
        }
        reads
    });

    p_handle.join().unwrap();
    let reads = c_handle.join().unwrap();
    assert!(reads > 0);
}
