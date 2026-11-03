//! Which song to align next. One at a time; the track that is playing goes
//! first and preempts a lookahead job; never more than `LOOKAHEAD` songs ahead.

use crate::types::LyricLine;
use std::collections::HashSet;

pub const LOOKAHEAD: usize = 3;

#[derive(Clone, Debug, PartialEq)]
pub struct Song {
    pub video_id: String,
    pub title: String,
    pub artist: String,
    /// Seconds, when known: guards against aligning a different upload.
    pub duration: Option<f64>,
}

/// The lyrics a job aligns: given for the playing track (the lookup already
/// ran), looked up by the worker for songs ahead.
#[derive(Clone, Debug)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    /// The lines carry a real source's timing (not an estimate).
    pub real_timing: bool,
}

#[derive(Clone, Debug)]
pub struct Job {
    pub song: Song,
    pub lyrics: Option<Lyrics>,
}

#[derive(Default)]
pub struct Queue {
    /// The playing track, while it still needs aligning.
    current: Option<Job>,
    /// Up next, in play order.
    ahead: Vec<Song>,
    /// Finished this session (aligned, rejected or failed): never retried.
    done: HashSet<String>,
    running: Option<String>,
}

impl Queue {
    /// The extension's view of what plays next. Replaces the old lookahead.
    pub fn set_upcoming(&mut self, songs: Vec<Song>) {
        let current = self.current.as_ref().map(|j| j.song.video_id.clone());
        let mut seen = HashSet::new();
        self.ahead = songs
            .into_iter()
            .filter(|s| Some(&s.video_id) != current.as_ref() && seen.insert(s.video_id.clone()))
            .take(LOOKAHEAD)
            .collect();
    }

    /// The playing track needs aligning (it was not in the cache).
    pub fn set_current(&mut self, job: Job) {
        if !self.done.contains(&job.song.video_id) {
            self.ahead.retain(|s| s.video_id != job.song.video_id);
            self.current = Some(job);
        }
    }

    /// Another track started: whatever the old one needed is moot.
    pub fn playing(&mut self, video_id: &str) {
        if self.current.as_ref().is_some_and(|j| j.song.video_id != video_id) {
            self.current = None;
        }
    }

    /// Should the running job make way for the playing track?
    pub fn should_preempt(&self) -> bool {
        match (&self.running, &self.current) {
            (Some(running), Some(cur)) => *running != cur.song.video_id,
            _ => false,
        }
    }

    /// The next job, marked running. The playing track first, then the lookahead in order.
    pub fn next(&mut self) -> Option<Job> {
        if self.running.is_some() {
            return None;
        }
        let job = match &self.current {
            Some(j) if !self.done.contains(&j.song.video_id) => j.clone(),
            _ => {
                let song = self.ahead.iter().find(|s| !self.done.contains(&s.video_id))?.clone();
                Job { song, lyrics: None }
            }
        };
        self.running = Some(job.song.video_id.clone());
        Some(job)
    }

    /// The running job ended. `finished: false` means it was preempted and may run again.
    pub fn end(&mut self, video_id: &str, finished: bool) {
        if self.running.as_deref() == Some(video_id) {
            self.running = None;
        }
        if finished {
            self.done.insert(video_id.to_string());
            if self.current.as_ref().is_some_and(|j| j.song.video_id == video_id) {
                self.current = None;
            }
            self.ahead.retain(|s| s.video_id != video_id);
        }
    }

    pub fn is_done(&self, video_id: &str) -> bool {
        self.done.contains(video_id)
    }

    #[cfg(test)]
    pub fn is_idle(&self) -> bool {
        self.running.is_none()
            && self.current.as_ref().is_none_or(|j| self.done.contains(&j.song.video_id))
            && self.ahead.iter().all(|s| self.done.contains(&s.video_id))
    }

    /// Forget everything (the feature was switched off).
    pub fn clear(&mut self) {
        let running = self.running.take();
        *self = Queue { running, ..Queue::default() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str) -> Song {
        Song { video_id: id.into(), title: id.into(), artist: "a".into(), duration: None }
    }

    fn ids(songs: &[&str]) -> Vec<Song> {
        songs.iter().map(|s| song(s)).collect()
    }

    #[test]
    fn lookahead_runs_in_play_order_and_at_most_three_ahead() {
        let mut q = Queue::default();
        q.set_upcoming(ids(&["b", "c", "d", "e"]));
        let mut order = Vec::new();
        while let Some(job) = q.next() {
            order.push(job.song.video_id.clone());
            q.end(&job.song.video_id, true);
        }
        assert_eq!(order, ["b", "c", "d"]);
        assert!(q.is_idle());
    }

    #[test]
    fn one_job_at_a_time() {
        let mut q = Queue::default();
        q.set_upcoming(ids(&["b", "c"]));
        assert!(q.next().is_some());
        assert!(q.next().is_none(), "b is still running");
    }

    #[test]
    fn the_playing_track_goes_first_and_preempts() {
        let mut q = Queue::default();
        q.set_upcoming(ids(&["b", "c"]));
        let running = q.next().unwrap();
        assert_eq!(running.song.video_id, "b");
        assert!(!q.should_preempt());

        q.set_current(Job { song: song("a"), lyrics: None });
        assert!(q.should_preempt());
        q.end("b", false); // preempted, not finished
        assert_eq!(q.next().unwrap().song.video_id, "a");
        q.end("a", true);
        // b comes back after the playing track.
        assert_eq!(q.next().unwrap().song.video_id, "b");
    }

    #[test]
    fn finished_songs_are_not_retried_and_skipping_drops_the_old_current() {
        let mut q = Queue::default();
        q.set_current(Job { song: song("a"), lyrics: None });
        let job = q.next().unwrap();
        q.end(&job.song.video_id, true);
        q.set_current(Job { song: song("a"), lyrics: None });
        assert!(q.next().is_none(), "a already done");

        q.set_current(Job { song: song("x"), lyrics: None });
        q.playing("y");
        assert!(q.next().is_none(), "x is no longer playing");
        // The playing track is never queued as lookahead too.
        q.set_current(Job { song: song("y"), lyrics: None });
        q.set_upcoming(ids(&["y", "z", "z"]));
        assert_eq!(q.next().unwrap().song.video_id, "y");
        q.end("y", true);
        assert_eq!(q.next().unwrap().song.video_id, "z");
        q.end("z", true);
        assert!(q.next().is_none());
    }
}
