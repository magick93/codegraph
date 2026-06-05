use std::collections::{HashMap, HashSet, VecDeque};

/// Compute topological ordering of view containers.
///
/// Views that are navigated TO come before views that navigate FROM them.
/// This ensures route files exist before they're imported/referenced.
///
/// Each entry in `navigation_edges` is a `(source, target)` pair indicating
/// that `source` navigates to `target`.  The returned order places targets
/// before sources so that generated route files are available before they
/// are imported by upstream views.
pub fn compute_view_generation_order(navigation_edges: &[(String, String)]) -> Vec<String> {
    // Build adjacency and in-degree
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_views: HashSet<String> = HashSet::new();

    for (source, target) in navigation_edges {
        // Edge direction: source navigates TO target.
        // For generation we want targets first (they have no dependencies
        // on sources), so we reverse the edge: target -> source in adj list.
        adjacency.entry(target.clone()).or_default().push(source.clone());
        *in_degree.entry(source.clone()).or_insert(0) += 1;
        in_degree.entry(target.clone()).or_insert(0);

        all_views.insert(source.clone());
        all_views.insert(target.clone());
    }

    // Kahn's algorithm — seed queue with zero-in-degree views in alphabetical order
    // for deterministic output across runs.
    let mut zero_in_degree: Vec<String> = all_views
        .iter()
        .filter(|v| in_degree.get(*v).copied().unwrap_or(0) == 0)
        .cloned()
        .collect();
    zero_in_degree.sort();
    let mut queue: VecDeque<String> = zero_in_degree.into_iter().collect();

    let mut order = Vec::new();
    while let Some(view) = queue.pop_front() {
        order.push(view.clone());

        if let Some(neighbors) = adjacency.get(&view) {
            let mut newly_zero: Vec<String> = Vec::new();
            for neighbor in neighbors {
                if let Some(degree) = in_degree.get_mut(neighbor) {
                    *degree -= 1;
                    if *degree == 0 {
                        newly_zero.push(neighbor.clone());
                    }
                }
            }
            newly_zero.sort();
            queue.extend(newly_zero);
        }
    }

    // If there are cycles, append remaining views in alphabetical order.
    if order.len() < all_views.len() {
        let ordered: HashSet<String> = order.iter().cloned().collect();
        let mut remaining: Vec<String> = all_views
            .into_iter()
            .filter(|v| !ordered.contains(v))
            .collect();
        remaining.sort();
        order.extend(remaining);
    }

    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_view_no_edges() {
        let edges = vec![];
        let order = compute_view_generation_order(&edges);
        assert!(order.is_empty());
    }

    #[test]
    fn test_two_view_chain() {
        let edges = vec![("List".to_string(), "Detail".to_string())];
        let order = compute_view_generation_order(&edges);
        // Detail has 0 in-degree → first, then List
        assert_eq!(order, vec!["Detail", "List"]);
    }

    #[test]
    fn test_linear_chain() {
        let edges = vec![
            ("A".to_string(), "B".to_string()),
            ("B".to_string(), "C".to_string()),
            ("C".to_string(), "D".to_string()),
        ];
        let order = compute_view_generation_order(&edges);
        // D (in-degree 0) → C → B → A
        assert_eq!(order, vec!["D", "C", "B", "A"]);
    }

    #[test]
    fn test_diamond() {
        let edges = vec![
            ("Root".to_string(), "Left".to_string()),
            ("Root".to_string(), "Right".to_string()),
            ("Left".to_string(), "Leaf".to_string()),
            ("Right".to_string(), "Leaf".to_string()),
        ];
        let order = compute_view_generation_order(&edges);
        // Leaf (0) first, then Left/Right (both 0 after Leaf removed),
        // then Root last.
        assert_eq!(order[0], "Leaf");
        assert!(order.contains(&"Left".to_string()));
        assert!(order.contains(&"Right".to_string()));
        assert_eq!(order[3], "Root");
    }

    #[test]
    fn test_cycle_does_not_infinite_loop() {
        let edges = vec![
            ("A".to_string(), "B".to_string()),
            ("B".to_string(), "C".to_string()),
            ("C".to_string(), "A".to_string()),
        ];
        let order = compute_view_generation_order(&edges);
        // All nodes have in-degree >= 1 in the cycle,
        // so Kahn's algorithm produces empty + fallback alphabetical.
        assert_eq!(order.len(), 3);
        assert_eq!(order, vec!["A", "B", "C"]);
    }

    #[test]
    fn test_disconnected_views() {
        let mut edges = Vec::new();
        for (s, t) in &[
            ("A".to_string(), "B".to_string()),
            ("C".to_string(), "D".to_string()),
            ("E".to_string(), "F".to_string()),
        ] {
            edges.push((s.clone(), t.clone()));
        }
        let order = compute_view_generation_order(&edges);
        // Targets (B, D, F) have in-degree 0 → come first alphabetically
        assert_eq!(order[0], "B");
        assert_eq!(order[1], "D");
        assert_eq!(order[2], "F");
        // Then sources in alphabetical order
        assert_eq!(order[3], "A");
        assert_eq!(order[4], "C");
        assert_eq!(order[5], "E");
    }
}
