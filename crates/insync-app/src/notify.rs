//! The notification service: throttle, do-not-disturb, optional sound and
//! the in-app notification list shown in the hub. One notification per
//! logical event, never a strobe.

use crate::store::Settings;
use bongocat_audio::MotionAudioClient;
use insync_core::UnixMs;
use std::collections::VecDeque;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub key: String,
    pub title: String,
    pub body: String,
    pub at: UnixMs,
}

/// Minimum spacing between two notifications of the same key.
const SAME_KEY_COOLDOWN_MS: i64 = 3000;
/// The in-app list keeps this many entries.
const HISTORY_CAP: usize = 30;

pub struct NotificationCenter {
    history: Mutex<VecDeque<Notification>>,
    last_sent_at: Mutex<Option<(String, UnixMs)>>,
    audio: MotionAudioClient,
    /// Bumped whenever a notification is added so the UI can refresh.
    version: std::sync::atomic::AtomicU64,
}

impl NotificationCenter {
    pub fn new(audio: MotionAudioClient) -> Self {
        Self {
            history: Mutex::new(VecDeque::new()),
            last_sent_at: Mutex::new(None),
            audio,
            version: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn version(&self) -> u64 {
        self.version.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn history(&self) -> Vec<Notification> {
        self.history
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }

    pub fn unread_count(&self) -> usize {
        self.history
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    pub fn clear(&self) {
        self.history
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        self.version
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Offers a notification. Returns `Some(notification)` when it was
    /// accepted (not throttled, not in DND for non-critical keys).
    pub fn offer(
        &self,
        key: &str,
        title: String,
        body: String,
        settings: &Settings,
        now: UnixMs,
    ) -> Option<Notification> {
        if !settings.notifications_enabled {
            return None;
        }
        if settings.dnd_enabled && key != "alarm" {
            return None;
        }
        {
            let last = self
                .last_sent_at
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some((last_key, last_at)) = last.as_ref()
                && last_key == key
                && now - *last_at < SAME_KEY_COOLDOWN_MS
            {
                return None;
            }
        }
        *self
            .last_sent_at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((key.to_string(), now));
        let notification = Notification {
            key: key.to_string(),
            title,
            body,
            at: now,
        };
        {
            let mut history = self
                .history
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            history.push_front(notification.clone());
            while history.len() > HISTORY_CAP {
                history.pop_back();
            }
        }
        self.version
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if settings.notification_sound
            && let Some(path) = crate::assets::alarm_sound_path()
        {
            let _ = self.audio.try_publish_with_sequence(|sequence| {
                bongocat_audio::MotionAudioCommand::Play {
                    sequence,
                    path: path.clone(),
                    volume: bongocat_audio::MotionAudioVolume::new(0.7)
                        .unwrap_or(bongocat_audio::MotionAudioVolume::FULL),
                }
            });
        }
        Some(notification)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Settings;

    fn center() -> NotificationCenter {
        NotificationCenter::new(bongocat_audio::MotionAudioClient::unavailable())
    }

    #[test]
    fn respects_disabled_and_dnd() {
        let center = center();
        let settings = Settings {
            notifications_enabled: false,
            ..Settings::default()
        };
        assert!(
            center
                .offer("msg", "t".into(), "b".into(), &settings, 1000)
                .is_none()
        );
        let settings = Settings {
            notifications_enabled: true,
            dnd_enabled: true,
            ..Settings::default()
        };
        assert!(
            center
                .offer("msg", "t".into(), "b".into(), &settings, 1000)
                .is_none()
        );
        // alarms break through DND
        assert!(
            center
                .offer("alarm", "t".into(), "b".into(), &settings, 1000)
                .is_some()
        );
    }

    #[test]
    fn throttles_same_key_but_not_different_keys() {
        let center = center();
        let settings = Settings::default();
        assert!(
            center
                .offer("poke", "t".into(), "b1".into(), &settings, 1000)
                .is_some()
        );
        assert!(
            center
                .offer("poke", "t".into(), "b2".into(), &settings, 2000)
                .is_none(),
            "same key within 3s is dropped"
        );
        assert!(
            center
                .offer("msg", "t".into(), "b3".into(), &settings, 2000)
                .is_some()
        );
        assert!(
            center
                .offer("poke", "t".into(), "b4".into(), &settings, 5000)
                .is_some(),
            "after the cooldown it passes again"
        );
        assert_eq!(center.history().len(), 3);
    }

    #[test]
    fn history_is_capped() {
        let center = center();
        let settings = Settings::default();
        for i in 0..40 {
            let _ = center.offer(
                &format!("key{i}"),
                "t".into(),
                format!("b{i}"),
                &settings,
                i as i64 * 10_000,
            );
        }
        assert_eq!(center.history().len(), 30);
        assert_eq!(center.history()[0].key, "key39");
    }
}
