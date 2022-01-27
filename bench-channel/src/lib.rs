#![feature(test)]

extern crate test;

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    const COUNT: i32 = 1000;

    #[bench]
    fn bench_crossbeam(b: &mut test::Bencher) {
        b.iter(|| {
            let (sender, receiver) = crossbeam_channel::unbounded();

            std::thread::spawn(move || {
                for i in 0..COUNT {
                    let s = format!("{}", i);
                    let _ = sender.send(s);
                }
            });

            let h2 = std::thread::spawn(move || {
                let mut sum: i64 = 0;
                while let Ok(s) = receiver.recv() {
                    let value = s.parse::<i32>().unwrap();
                    sum += value as i64;
                }

                sum
            });

            let v = h2.join().unwrap();
            assert_eq!(v, 499500);
        });
    }

    #[bench]
    fn bench_std_mpsc(b: &mut test::Bencher) {
        b.iter(|| {
            let (sender, receiver) = std::sync::mpsc::channel();

            std::thread::spawn(move || {
                for i in 0..COUNT {
                    let s = format!("{}", i);
                    let _ = sender.send(s);
                }
            });

            let h2 = std::thread::spawn(move || {
                let mut sum: i64 = 0;
                while let Ok(s) = receiver.recv() {
                    let value = s.parse::<i32>().unwrap();
                    sum += value as i64;
                }

                sum
            });

            let v = h2.join().unwrap();
            assert_eq!(v, 499500);
        });
    }

    #[bench]
    fn bench_arc_mutex(b: &mut test::Bencher) {
        b.iter(|| {
            // let queue = Arc::new(Mutex::new(VecDeque::new()));
            let queue = Arc::new(Mutex::new(VecDeque::with_capacity(COUNT as usize)));

            let send_queue = queue.clone();
            std::thread::spawn(move || {
                for i in 0..COUNT {
                    let s = format!("{}", i);
                    let mut locked = send_queue.lock().unwrap();
                    locked.push_back(s);
                    drop(locked);
                }
            });

            let t2 = std::thread::spawn(move || {
                let mut sum: i64 = 0;
                for _ in 0..COUNT {
                    loop {
                        let mut locked = queue.lock().unwrap();
                        if let Some(s) = locked.pop_front() {
                            let value = s.parse::<i32>().unwrap();
                            sum += value as i64;
                            break;
                        }
                        drop(locked);
                    }
                }

                sum
            });

            let sum = t2.join().unwrap();
            assert_eq!(sum, 499500);
        });
    }

    #[bench]
    fn bench_arc_parkinglot_mutex(b: &mut test::Bencher) {
        b.iter(|| {
            let queue = Arc::new(parking_lot::Mutex::new(VecDeque::with_capacity(
                COUNT as usize,
            )));

            let send_queue = queue.clone();
            std::thread::spawn(move || {
                for i in 0..COUNT {
                    let s = format!("{}", i);
                    let mut locked = send_queue.lock();
                    locked.push_back(s);
                    drop(locked);
                }
            });

            let t2 = std::thread::spawn(move || {
                let mut sum: i64 = 0;
                for _ in 0..COUNT {
                    loop {
                        let mut locked = queue.lock();
                        if let Some(s) = locked.pop_front() {
                            let value = s.parse::<i32>().unwrap();
                            sum += value as i64;
                            break;
                        }
                        drop(locked);
                    }
                }

                sum
            });

            let sum = t2.join().unwrap();
            assert_eq!(sum, 499500);
        });
    }

    #[bench]
    fn bench_arc_parkinglot_rwlock(b: &mut test::Bencher) {
        b.iter(|| {
            // let queue = Arc::new(parking_lot::RwLock::new(VecDeque::new()));
            let queue = Arc::new(parking_lot::RwLock::new(VecDeque::with_capacity(
                COUNT as usize,
            )));

            let send_queue = queue.clone();
            std::thread::spawn(move || {
                for i in 0..COUNT {
                    let s = format!("{}", i);
                    let mut locked = send_queue.write();
                    locked.push_back(s);
                    drop(locked);
                }
            });

            let t2 = std::thread::spawn(move || {
                let mut sum: i64 = 0;
                for _ in 0..COUNT {
                    loop {
                        let mut locked = queue.write();
                        if let Some(s) = locked.pop_front() {
                            let value = s.parse::<i32>().unwrap();
                            sum += value as i64;
                            break;
                        }
                        drop(locked);
                    }
                }

                sum
            });

            let sum = t2.join().unwrap();
            assert_eq!(sum, 499500);
        });
    }
}
