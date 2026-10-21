use super::*;
use proptest::prelude::*;

proptest! {
    // #58 "Minimal copy" — every reserved run comes back whole and in order,
    // and a lane that never holds more than `most` bytes stops growing
    #[test]
    fn order(ops in prop::collection::vec((1usize..300, 0usize..4), 1..400)) {
        let most = 1200;
        let mut lane = Lane::default();
        let mut queued: std::collections::VecDeque<Vec<u8>> = Default::default();
        let mut seq = 0u8;
        for &(n, takes) in &ops {
            if lane.len() + n <= most {
                let run: Vec<u8> = (0..n).map(|_| { seq = seq.wrapping_add(1); seq }).collect();
                lane.reserve(n).copy_from_slice(&run);
                queued.push_back(run);
            }
            for _ in 0..takes {
                let Some(run) = queued.pop_front() else { break };
                prop_assert_eq!(&lane.front()[..run.len()], &run[..]);
                let range = lane.take(run.len());
                prop_assert_eq!(lane.slice(range), &run[..]);
            }
            prop_assert_eq!(lane.len(), queued.iter().map(Vec::len).sum::<usize>());
        }
        let bound = 2 * (2 * most + 2 * 300); // room never runs out past twice the content plus two runs
        prop_assert!(lane.buf.len() <= bound, "grew to {}", lane.buf.len());
    }
}
