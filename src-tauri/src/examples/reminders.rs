//! Example: Apple Reminders (EventKit). Open reminders from every list, what
//! you finished today, and check off / un-check / add. macOS asks once for
//! Reminders access (`reminders_request_access`). Needs the Reminders keys in
//! Info.plist and Entitlements.plist.

use serde::Serialize;

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: String,
    pub title: String,
    /// The Reminders list it's in ("Groceries", "Work"…).
    pub list: String,
    /// "YYYY-MM-DD", if it has a due date.
    pub due: Option<String>,
    /// "HH:MM" (24h), if the due date has a time.
    pub due_time: Option<String>,
    /// 0 none, 1 high … 9 low (Apple's scale).
    pub priority: u64,
    pub notes: Option<String>,
    pub completed: bool,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    Granted,
    Denied,
    Unknown,
}

/// Open reminders first: overdue and due ones by date, then undated by list.
pub fn sort(list: &mut [Reminder]) {
    list.sort_by(|a, b| {
        a.completed
            .cmp(&b.completed)
            .then_with(|| match (&a.due, &b.due) {
                (Some(x), Some(y)) => x.cmp(y).then(a.due_time.cmp(&b.due_time)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.list.cmp(&b.list),
            })
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::Bool;
    use objc2_event_kit::{EKAuthorizationStatus, EKEntityType, EKEventStore, EKReminder};
    use objc2_foundation::{NSArray, NSDate, NSError, NSString};
    use std::sync::mpsc;
    use std::time::Duration;

    /// NSDateComponentUndefined
    const UNDEF: isize = isize::MAX;

    pub fn access() -> Access {
        let s = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Reminder) };
        if s == EKAuthorizationStatus::FullAccess {
            Access::Granted
        } else if s == EKAuthorizationStatus::NotDetermined {
            Access::Unknown
        } else {
            Access::Denied
        }
    }

    pub fn request_access() -> Access {
        let store = unsafe { EKEventStore::new() };
        let (tx, rx) = mpsc::channel();
        let block = RcBlock::new(move |_granted: Bool, _err: *mut NSError| {
            let _ = tx.send(());
        });
        unsafe { store.requestFullAccessToRemindersWithCompletion(&*block as *const _ as *mut _) };
        let _ = rx.recv_timeout(Duration::from_secs(120));
        let a = access();
        crate::log(&format!("reminders: access after prompt: {a:?}"));
        a
    }

    fn to_reminder(r: &EKReminder) -> Reminder {
        unsafe {
            let (due, due_time) = match r.dueDateComponents() {
                Some(c) if c.year() != UNDEF && c.month() != UNDEF && c.day() != UNDEF => (
                    Some(format!("{:04}-{:02}-{:02}", c.year(), c.month(), c.day())),
                    (c.hour() != UNDEF).then(|| format!("{:02}:{:02}", c.hour(), if c.minute() == UNDEF { 0 } else { c.minute() })),
                ),
                _ => (None, None),
            };
            Reminder {
                id: r.calendarItemIdentifier().to_string(),
                title: r.title().to_string(),
                list: r.calendar().map(|c| c.title().to_string()).unwrap_or_default(),
                due,
                due_time,
                priority: r.priority() as u64,
                notes: r.notes().map(|n| n.to_string()).filter(|n| !n.trim().is_empty()),
                completed: r.isCompleted(),
            }
        }
    }

    fn fetch(store: &EKEventStore, pred: &objc2_foundation::NSPredicate) -> Vec<Reminder> {
        let (tx, rx) = mpsc::channel::<Vec<Reminder>>();
        let started = std::time::Instant::now();
        let block = RcBlock::new(move |arr: *mut NSArray<EKReminder>| {
            if arr.is_null() {
                crate::log("reminders: EventKit returned nil (fetch failed)");
            }
            let list = unsafe { arr.as_ref() }.map(|a| a.iter().map(|r| to_reminder(&r)).collect()).unwrap_or_default();
            let _ = tx.send(list);
        });
        unsafe { store.fetchRemindersMatchingPredicate_completion(pred, &block) };
        match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(list) => list,
            Err(_) => {
                crate::log(&format!("reminders: fetch timed out after {:?}", started.elapsed()));
                vec![]
            }
        }
    }

    /// Open reminders, plus the ones finished today.
    pub fn list() -> Vec<Reminder> {
        let raw = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Reminder) };
        if access() != Access::Granted {
            crate::log(&format!("reminders: not allowed (status {})", raw.0));
            return vec![];
        }
        let store = unsafe { EKEventStore::new() };
        let open = unsafe { store.predicateForIncompleteRemindersWithDueDateStarting_ending_calendars(None, None, None) };
        let midnight = chrono::Local::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
        let start = chrono::TimeZone::from_local_datetime(&chrono::Local, &midnight).single().map(|d| d.timestamp()).unwrap_or(0);
        let since = NSDate::dateWithTimeIntervalSince1970(start as f64);
        let done = unsafe { store.predicateForCompletedRemindersWithCompletionDateStarting_ending_calendars(Some(&since), None, None) };
        let mut all = fetch(&store, &open);
        all.extend(fetch(&store, &done));
        sort(&mut all);
        all
    }

    fn find(store: &EKEventStore, id: &str) -> Result<Retained<EKReminder>, String> {
        let item = unsafe { store.calendarItemWithIdentifier(&NSString::from_str(id)) }.ok_or("That reminder no longer exists")?;
        item.downcast::<EKReminder>().map_err(|_| "Not a reminder".to_string())
    }

    pub fn set_completed(id: &str, done: bool) -> Result<(), String> {
        let store = unsafe { EKEventStore::new() };
        let r = find(&store, id)?;
        unsafe {
            r.setCompleted(done);
            store.saveReminder_commit_error(&r, true).map_err(|e| e.localizedDescription().to_string())
        }
    }

    /// Add a reminder to your default Reminders list.
    pub fn add(title: &str) -> Result<Reminder, String> {
        let store = unsafe { EKEventStore::new() };
        unsafe {
            let cal = store.defaultCalendarForNewReminders().ok_or("No default Reminders list")?;
            let r = EKReminder::reminderWithEventStore(&store);
            r.setTitle(Some(&NSString::from_str(title)));
            r.setCalendar(Some(&cal));
            store.saveReminder_commit_error(&r, true).map_err(|e| e.localizedDescription().to_string())?;
            Ok(to_reminder(&r))
        }
    }
}

macro_rules! mac_or {
    ($mac:expr, $other:expr) => {{
        #[cfg(target_os = "macos")]
        {
            $mac
        }
        #[cfg(not(target_os = "macos"))]
        {
            $other
        }
    }};
}

pub fn access() -> Access {
    mac_or!(mac::access(), Access::Denied)
}
pub fn request_access() -> Access {
    mac_or!(mac::request_access(), Access::Denied)
}
pub fn list() -> Vec<Reminder> {
    mac_or!(mac::list(), vec![])
}
pub fn set_completed(id: &str, done: bool) -> Result<(), String> {
    mac_or!(mac::set_completed(id, done), { let _ = (id, done); Err("Reminders needs macOS".into()) })
}
pub fn add(title: &str) -> Result<Reminder, String> {
    mac_or!(mac::add(title), { let _ = title; Err("Reminders needs macOS".into()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(title: &str, due: Option<&str>, list: &str, completed: bool) -> Reminder {
        Reminder { id: title.into(), title: title.into(), list: list.into(), due: due.map(Into::into), due_time: None, priority: 0, notes: None, completed }
    }

    #[test]
    fn sorts_dated_then_undated_then_done() {
        let mut v = vec![r("done", None, "A", true), r("undated b", None, "B", false), r("later", Some("2026-11-10"), "A", false), r("soon", Some("2026-11-06"), "A", false), r("undated a", None, "A", false)];
        sort(&mut v);
        let t: Vec<_> = v.iter().map(|x| x.title.as_str()).collect();
        assert_eq!(t, ["soon", "later", "undated a", "undated b", "done"]);
    }
}
