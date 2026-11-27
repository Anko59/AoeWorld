use super::*;

#[test]
fn every_declared_name_stands_for_one_number_and_every_path_is_listed_once() {
    let mut names: Vec<&str> = KNOWN.iter().filter_map(|known| known.declare).collect();
    names.sort_unstable();
    let count = names.len();
    names.dedup();
    assert_eq!(names.len(), count, "a declaration name twice");
    let mut paths: Vec<&str> = KNOWN.iter().map(|known| known.path).collect();
    paths.sort_unstable();
    paths.dedup();
    assert_eq!(paths.len(), KNOWN.len(), "a path twice");
    for known in KNOWN {
        assert_eq!(
            known.declare.is_some(),
            known.better != Better::Neither,
            "{}: a number is judged when it has a better side",
            known.path
        );
    }
}

#[test]
fn a_number_is_found_by_its_declaration_name() {
    let known = by_declaration("first-step-p95").expect("known");
    assert_eq!(known.path, "arrival.order_to_first_step_ticks.p95");
    assert_eq!(known.better, Better::Lower);
    assert!(by_declaration("nothing").is_none());
    assert_eq!(
        by_declaration("arrived").map(|known| known.better),
        Some(Better::Higher)
    );
}

#[test]
fn values_read_in_words() {
    assert_eq!(show("arrival.arrived_permille", Some(971.0)), "97.1%");
    assert_eq!(show("size.units", Some(12_345.0)), "12,345");
    assert_eq!(show("size.units", Some(0.0)), "0");
    assert_eq!(show("efficiency.per_arrival", Some(0.57)), "0.57");
    assert_eq!(show("efficiency.per_hit", Some(269.9)), "269.9");
    assert_eq!(show("size.units", None), "none");
    assert_eq!(show("planning.stuck", Some(6383.5)), "6,384");
    assert_eq!(
        show("arrival.order_to_arrival_ticks.p95", Some(59.5)),
        "59.5"
    );
}

#[test]
fn a_path_the_table_does_not_know_is_its_own_label() {
    assert_eq!(label("planning.approached"), "planning.approached");
    assert_eq!(label("arrival.orders_given"), "movement orders given");
    assert_eq!(better("planning.approached"), Better::Neither);
}

#[test]
fn grouping_keeps_the_sign() {
    assert_eq!(grouped(1_234_567), "1,234,567");
    assert_eq!(grouped(-1_234), "-1,234");
    assert_eq!(grouped(999), "999");
}

#[test]
fn the_floor_of_a_move_is_a_few_ticks_for_a_delay_and_ten_for_the_rest() {
    for path in [
        "arrival.order_to_arrival_ticks.p95",
        "arrival.order_to_first_step_ticks.p95",
        "planning.wait_p95_ticks_peak",
    ] {
        assert_eq!(floor(path), 3.0, "{path}");
    }
    for path in [
        "planning.expired",
        "planning.stuck",
        "planning.waiting_peak",
        "arrival.arrived_permille",
        "size.idle_villagers_permille",
        "economy.delivered_total",
    ] {
        assert_eq!(floor(path), 10.0, "{path}");
    }
}

#[test]
fn the_names_a_declaration_may_use_are_listed_in_the_order_of_the_table() {
    let names = declarable();
    assert_eq!(names.first(), Some(&"idle"));
    assert_eq!(names.len(), 13);
    assert!(names.contains(&"cost-per-unit"));
}
