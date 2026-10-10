//! Client-side sync semantics: reconnect backoff, event de-duplication,
//! message cache merging and unread counting. All pure and unit-tested.

use crate::{Message, UnixMs};
use std::collections::HashSet;

/// Reconnect backoff with a 1s base, doubling to a 60s cap. `attempt` is
/// 0-based (the first retry uses the base delay).
pub fn reconnect_delay_ms(attempt: u32) -> i64 {
    let shift = attempt.min(6);
    (1000i64 << shift).min(60_000)
}

/// Remembers processed event ids so a redelivery can never trigger a second
/// interaction. The set is capped; the oldest ids are forgotten first, which
/// is safe because redeliveries only happen within short reconnect windows.
#[derive(Debug, Default)]
pub struct EventDeduper {
    seen: HashSet<String>,
    order: std::collections::VecDeque<String>,
    capacity: usize,
}

impl EventDeduper {
    pub fn new(capacity: usize) -> Self {
        Self {
            seen: HashSet::new(),
            order: std::collections::VecDeque::new(),
            capacity: capacity.max(8),
        }
    }

    /// Returns true when this event was already processed.
    pub fn is_duplicate(&mut self, id: &str) -> bool {
        if !self.seen.insert(id.to_string()) {
            return true;
        }
        self.order.push_back(id.to_string());
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.seen.remove(&oldest);
            }
        }
        false
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

/// The local message cache: sorted by seq, deduped by seq and id, with a
/// bounded size (oldest dropped first) and a cursor of the highest seq seen.
#[derive(Debug, Default)]
pub struct MessageCache {
    messages: Vec<Message>,
    capacity: usize,
    highest_seq: u64,
}

impl MessageCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            messages: Vec::new(),
            capacity: capacity.max(1),
            highest_seq: 0,
        }
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn highest_seq(&self) -> u64 {
        self.highest_seq
    }

    /// Inserts messages from a batch (event push or REST page). Returns the
    /// ones that were actually new, in seq order. Duplicates inside one
    /// batch are collapsed too.
    pub fn merge(&mut self, incoming: Vec<Message>) -> Vec<Message> {
        let mut fresh: Vec<Message> = Vec::new();
        for m in incoming {
            if m.seq > self.highest_seq {
                self.highest_seq = m.seq;
            }
            // dedupe by seq (primary) — server guarantees seq uniqueness
            if self.messages.iter().any(|e| e.seq == m.seq) || fresh.iter().any(|e| e.seq == m.seq)
            {
                continue;
            }
            // id collision with a different seq would be a protocol bug, but
            // guard anyway so the cache can never show one message twice
            if self.messages.iter().any(|e| e.id == m.id) || fresh.iter().any(|e| e.id == m.id) {
                continue;
            }
            fresh.push(m);
        }
        if fresh.is_empty() {
            return fresh;
        }
        self.messages.extend(fresh.iter().cloned());
        self.messages.sort_by_key(|m| m.seq);
        while self.messages.len() > self.capacity {
            self.messages.remove(0);
        }
        fresh
    }

    /// Unread count for `my_id` given my read watermark.
    pub fn unread_from_partner(&self, my_id: &str, my_read_seq: u64) -> u64 {
        self.messages
            .iter()
            .filter(|m| m.sender_id != my_id && m.seq > my_read_seq)
            .count() as u64
    }

    /// The last message regardless of sender (recent-conversation preview).
    pub fn last(&self) -> Option<&Message> {
        self.messages.last()
    }

    /// Clears the cache (e.g. after the partner account data was cleared);
    /// the cursor is kept so catch-up continues where it stopped.
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&CachedMessages {
            messages: self.messages.clone(),
            highest_seq: self.highest_seq,
        })
        .expect("cache serialization cannot fail")
    }

    pub fn from_json(json: &str, capacity: usize) -> Result<Self, serde_json::Error> {
        let cached: CachedMessages = serde_json::from_str(json)?;
        let mut out = Self::new(capacity);
        out.merge(cached.messages);
        out.highest_seq = out.highest_seq.max(cached.highest_seq);
        Ok(out)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CachedMessages {
    messages: Vec<Message>,
    highest_seq: u64,
}

/// Unread letter count for the overview badge: letters from the partner that
/// have no read timestamp yet.
pub fn unread_letters(letters: &[crate::Letter], my_id: &str) -> u64 {
    letters
        .iter()
        .filter(|l| l.sender_id != my_id && l.read_at.is_none())
        .count() as u64
}

/// Pokes received within the recent window, capped so a reconnected user
/// sees at most a light reminder, never a replay of stale animations.
pub fn recent_pokes(pokes: &[crate::PokeRecord], my_id: &str, now: UnixMs) -> u64 {
    pokes
        .iter()
        .filter(|p| {
            p.sender_id != my_id
                && now - p.created_at <= crate::limits::RECENT_INTERACTION_WINDOW_MS
        })
        .count()
        .min(3) as u64
}

/// Whether a partner presence should currently render as "online".
pub fn partner_is_online(presence: Option<&crate::Presence>, now: UnixMs) -> bool {
    presence.is_some_and(|p| p.online && now - p.last_seen_at <= 45_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EventFrame, EventKind};

    fn msg(seq: u64, sender: &str) -> Message {
        Message {
            seq,
            id: format!("m{seq}"),
            sender_id: sender.into(),
            body: format!("body {seq}"),
            created_at: 1_700_000_000_000 + seq as i64,
        }
    }

    #[test]
    fn reconnect_backoff_caps_at_60s() {
        assert_eq!(reconnect_delay_ms(0), 1000);
        assert_eq!(reconnect_delay_ms(1), 2000);
        assert_eq!(reconnect_delay_ms(3), 8000);
        assert_eq!(reconnect_delay_ms(10), 60_000);
        assert_eq!(reconnect_delay_ms(50), 60_000);
    }

    #[test]
    fn deduper_blocks_second_delivery_and_forgives_old_ids() {
        let mut d = EventDeduper::new(8);
        assert!(!d.is_duplicate("a"));
        assert!(d.is_duplicate("a"));
        for id in ["b", "c", "d", "e", "f", "g", "h", "i"] {
            assert!(!d.is_duplicate(id));
        }
        // "a" was evicted (capacity 8) so it may be processed again
        assert!(!d.is_duplicate("a"));
        assert_eq!(d.len(), 8);
    }

    #[test]
    fn merge_dedupes_and_sorts() {
        let mut cache = MessageCache::new(16);
        let first = cache.merge(vec![msg(2, "a"), msg(1, "a"), msg(2, "a")]);
        assert_eq!(first.len(), 2);
        assert_eq!(
            cache.messages().iter().map(|m| m.seq).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(cache.highest_seq(), 2);
        // out-of-order catch-up inserts in place
        cache.merge(vec![msg(5, "b"), msg(3, "a")]);
        assert_eq!(
            cache.messages().iter().map(|m| m.seq).collect::<Vec<_>>(),
            vec![1, 2, 3, 5]
        );
        assert_eq!(
            cache.unread_from_partner("a", 2),
            1,
            "only msg5 from b is unread"
        );
        assert_eq!(
            cache.unread_from_partner("b", 0),
            3,
            "b has not read a's msgs"
        );
    }

    #[test]
    fn cache_bounded_and_persists() {
        let mut cache = MessageCache::new(4);
        cache.merge((1..=10).map(|s| msg(s, "a")).collect());
        assert_eq!(cache.messages().len(), 4);
        assert_eq!(cache.messages()[0].seq, 7);
        let json = cache.to_json();
        let restored = MessageCache::from_json(&json, 100).unwrap();
        assert_eq!(restored.messages().len(), 4);
        assert_eq!(restored.highest_seq(), 10);
    }

    #[test]
    fn unread_letters_counts_partner_only() {
        let letters = vec![
            crate::Letter {
                id: "1".into(),
                sender_id: "me".into(),
                body: "x".into(),
                created_at: 0,
                read_at: None,
            },
            crate::Letter {
                id: "2".into(),
                sender_id: "partner".into(),
                body: "y".into(),
                created_at: 0,
                read_at: None,
            },
            crate::Letter {
                id: "3".into(),
                sender_id: "partner".into(),
                body: "z".into(),
                created_at: 0,
                read_at: Some(5),
            },
        ];
        assert_eq!(unread_letters(&letters, "me"), 1);
    }

    #[test]
    fn recent_pokes_capped_and_windowed() {
        let now = 1_000_000_000;
        let pokes = vec![
            crate::PokeRecord {
                id: "1".into(),
                sender_id: "partner".into(),
                created_at: now - 100,
            },
            crate::PokeRecord {
                id: "2".into(),
                sender_id: "partner".into(),
                created_at: now - 200,
            },
            crate::PokeRecord {
                id: "3".into(),
                sender_id: "partner".into(),
                created_at: now - 300,
            },
            crate::PokeRecord {
                id: "4".into(),
                sender_id: "partner".into(),
                created_at: now - 400,
            },
            crate::PokeRecord {
                id: "5".into(),
                sender_id: "me".into(),
                created_at: now - 500,
            },
        ];
        assert_eq!(recent_pokes(&pokes, "me", now), 3, "capped at 3");
        let old = vec![crate::PokeRecord {
            id: "9".into(),
            sender_id: "partner".into(),
            created_at: now - crate::limits::RECENT_INTERACTION_WINDOW_MS - 1,
        }];
        assert_eq!(recent_pokes(&old, "me", now), 0);
    }

    #[test]
    fn partner_online_requires_fresh_heartbeat() {
        let now = 1_000_000;
        let fresh = crate::Presence {
            user_id: "p".into(),
            state: crate::PresenceState::Online,
            online: true,
            last_seen_at: now - 10_000,
            focus_session_id: None,
        };
        let stale = crate::Presence {
            last_seen_at: now - 100_000,
            ..fresh.clone()
        };
        let offline = crate::Presence {
            online: false,
            ..fresh.clone()
        };
        assert!(partner_is_online(Some(&fresh), now));
        assert!(!partner_is_online(Some(&stale), now));
        assert!(!partner_is_online(Some(&offline), now));
        assert!(!partner_is_online(None, now));
    }

    #[test]
    fn event_frames_dedupe_by_id_not_kind() {
        let mut d = EventDeduper::new(64);
        let f1 = EventFrame::new(
            1,
            "e1".into(),
            EventKind::PokeReceived,
            "a".into(),
            "r".into(),
            0,
            crate::EventData::Poke {
                poke_id: "p".into(),
            },
        );
        assert!(!d.is_duplicate(&f1.id));
        assert!(d.is_duplicate(&f1.id));
    }
}
