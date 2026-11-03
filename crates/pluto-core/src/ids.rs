//! Identifiers: UUIDv7 text (Schema §1) — unique, and sortable by creation time.

pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_sort_by_creation_time() {
        let ids: Vec<String> = (0..1000).map(|_| new_id()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
        let first = ids.first().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert!(&new_id() > first, "a later id sorts after an earlier one");
    }
}
