#[derive(Debug, Clone)]
pub struct Paging {
    pub total: u64,
    pub count: u64,
    pub offset: u64,
    pub limit: u64,
    pub has_more: bool,
    pub next_offset: Option<u64>,
    #[allow(dead_code)]
    pub prev_offset: Option<u64>,
}

pub fn build_paging(total: u64, count: u64, offset: u64, limit: u64) -> Paging {
    let has_more = offset + count < total;
    let next_offset = if has_more { Some(offset + limit) } else { None };
    let prev_offset = if offset > 0 {
        Some(offset.saturating_sub(limit))
    } else {
        None
    };

    Paging {
        total,
        count,
        offset,
        limit,
        has_more,
        next_offset,
        prev_offset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_paging() {
        let paging = build_paging(100, 10, 0, 10);
        assert!(paging.has_more);
        assert_eq!(paging.next_offset, Some(10));
        assert_eq!(paging.prev_offset, None);
    }

    #[test]
    fn no_more_pages() {
        let paging = build_paging(10, 10, 0, 10);
        assert!(!paging.has_more);
        assert_eq!(paging.next_offset, None);
    }
}
