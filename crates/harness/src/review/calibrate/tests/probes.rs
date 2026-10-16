//! Small domain probes for every Rust seed, run on both base and diff.
//! Checks explicitly pin planted defects and the counter base awaiting repair;
//! they are not correctness
//! claims for the intentionally broken cases or performance qualification.

pub(super) fn source(id: &str, post: bool) -> String {
    let (shared, base, diff) = match id {
        "clean-rust-add-missing-test" => (
            r#"assert_eq!(slug(""), ""); assert_eq!(slug("  ONE  Two "), "one-two"); assert_eq!(slug("ÉTÉ"), "été");"#,
            r#""#,
            r#""#,
        ),
        "clean-rust-error-enum" => (
            r#"assert_eq!(parse_level(" 1 "), Ok(1)); assert_eq!(parse_level("20"), Ok(20)); for s in ["", "x", "0", "21", "256"] { assert!(parse_level(s).is_err()); }"#,
            r#""#,
            r#""#,
        ),
        "clean-rust-extract-helper" => (
            r#"for (p,w,e) in [(0,1,0),(3,4,75),(-1,4,0),(8,4,100)] { assert_eq!(health_percent(p,w),e); assert_eq!(progress_percent(p,w),e); }"#,
            r#""#,
            r#""#,
        ),
        "clean-rust-feature-peek" => (
            r#"let mut s = Stack::new(); assert_eq!(s.pop(), None); s.push(1); s.push(2); assert_eq!(s.pop(), Some(2)); assert_eq!(s.pop(), Some(1));"#,
            r#""#,
            r#"let mut s = Stack::new(); assert_eq!(s.peek(), None); s.push(3); assert_eq!(s.peek(), Some(&3)); assert_eq!(s.peek(), Some(&3));"#,
        ),
        "clean-rust-feature-retries" => (
            r#"for t in [0,1,1000] { let c = Client::new().with_timeout(t); assert_eq!((c.timeout_ms,c.retries),(t,3)); }"#,
            r#""#,
            r#"for n in [0,1,9] { assert_eq!(Client::new().with_retries(n).retries,n); }"#,
        ),
        "clean-rust-iterator-refactor" => (
            r#"assert_eq!(mean(&[]),None); assert_eq!(mean(&[5.]),Some(5.)); assert_eq!(mean(&[-2.,2.]),Some(0.));"#,
            r#""#,
            r#""#,
        ),
        "clean-rust-rename" => (
            r#"assert_eq!(invoice_total(&[]),0); assert_eq!(invoice_total(&[(2,3),(4,2)]),14);"#,
            r#"assert_eq!(calc(0,3),0); assert_eq!(calc(7,3),21);"#,
            r#"assert_eq!(total_price(0,3),0); assert_eq!(total_price(7,3),21);"#,
        ),
        "correctness-byte-sum-overflow" => (
            r#"assert!(is_empty(&[])); assert!(!is_empty(&[0]));"#,
            r#""#,
            r#"assert_eq!(checksum(&[]),0); assert_eq!(checksum(&[1,2,3]),6); assert_eq!(checksum(&[255,1]),0);"#,
        ),
        "correctness-discount-rounding-order" => (
            r#"assert_eq!(add_tax(0,20),0); assert_eq!(add_tax(100,20),120);"#,
            r#""#,
            r#"assert_eq!(apply_discount(1000,25),750); assert_eq!(apply_discount(199,10),90); assert_eq!(apply_discount(99,0),0);"#,
        ),
        "correctness-inclusive-range-boundary" => (
            r#"for (x,e) in [(-1,0),(2,2),(9,5)] { assert_eq!(clamp_to(x,0,5),e); }"#,
            r#""#,
            r#"for (x,e) in [(-1,false),(0,false),(1,true),(5,false),(6,false)] { assert_eq!(in_range(x,0,5),e); }"#,
        ),
        "correctness-window-sums-off-by-one" => (
            r#"assert_eq!(total(&[]),0); assert_eq!(total(&[-2,3]),1);"#,
            r#""#,
            r#"assert_eq!(window_sums(&[],1),vec![]); assert_eq!(window_sums(&[1,2,3],2),vec![3]); assert_eq!(window_sums(&[1,2],2),vec![]); assert_eq!(window_sums(&[1],0),vec![]);"#,
        ),
        "clean-hard-best-effort-send" => (
            r#"struct Good; impl Sink for Good { fn send(&self,_:&str)->Result<(),String>{Ok(())} } let g=Good; let mut r=Recorder::new(&g); assert_eq!(r.lines,0);"#,
            r#"r.record("").unwrap(); r.record("x").unwrap(); assert_eq!(r.lines,2);"#,
            r#"r.record(""); r.record("x"); assert_eq!(r.lines,2); struct Bad; impl Sink for Bad {fn send(&self,_:&str)->Result<(),String>{Err("full".into())}} let b=Bad; let mut r=Recorder::new(&b); r.record("x"); assert_eq!(r.lines,1);"#,
        ),
        "clean-hard-mechanical-rename" => (
            r#""#,
            r#"use seed::{herd::Herd,meadow::Meadow}; let h=Herd::new(2); let mut m=Meadow::new(3); assert_eq!(h.appetite(),4); assert_eq!(h.graze(&mut m),3); assert_eq!(m.grass(),0); assert_eq!(h.graze(&mut m),0);"#,
            r#"use seed::{herd::Flock,meadow::Pasture}; let h=Flock::new(2); let mut m=Pasture::new(3); assert_eq!(h.appetite(),4); assert_eq!(h.graze(&mut m),3); assert_eq!(m.grass(),0); assert_eq!(h.graze(&mut m),0);"#,
        ),
        "clean-hard-removed-port-check" => (
            r#"assert!(connect("localhost",1024).is_ok()); assert!(connect("localhost",65535).is_ok());"#,
            r#"assert!(connect("localhost",80).is_err()); assert!(connect("localhost",443).is_err());"#,
            r#"assert!(connect("localhost",80).is_ok()); assert!(connect("localhost",443).is_ok());"#,
        ),
        "clean-hard-sum-equivalence" => (
            r#"for n in [0,1,2,9,60000] { assert_eq!(caravan_gold(n),(1..=n).sum::<u32>()); }"#,
            r#""#,
            r#""#,
        ),
        "clean-hard-wrapping-sequence" => (
            r#"let mut s=Sender::new(); assert_eq!(s.next(),0); for i in 0..255 { assert_eq!(s.take(),i); } assert_eq!(s.next(),255);"#,
            r#""#,
            r#"assert_eq!(s.take(),255); assert_eq!(s.take(),0); assert_eq!(s.next(),1);"#,
        ),
        "hard-chunk-span-off-by-one" => (
            r#"use seed::{grid, row::Row}; let r=Row::new(&[0,1,2,3,4,5,6,7,8,9,10,11]); assert_eq!(r.prefix(0),&[]); assert_eq!(r.prefix(1),&[0,1,2,3]); assert_eq!(grid::first_tile(3),12);"#,
            r#""#,
            r#"assert_eq!(grid::tile_count(0,0),1); assert_eq!(r.chunk_span(0,1),&[0,1,2,3,4]); assert!(std::panic::catch_unwind(||r.chunk_span(2,3)).is_err());"#,
        ),
        "hard-hashmap-tie-break" => (
            r#"let mut b=Board::new(); assert_eq!(b.winner(),None); b.add(9,1); b.add(2,3); assert_eq!(b.winner(),Some(2)); assert_eq!(b.ids(),vec![2,9]);"#,
            r#"b.add(9,3); assert_eq!(b.winner(),Some(2));"#,
            r#"b.add(9,3); assert!([Some(2),Some(9)].contains(&b.winner()));"#,
        ),
        "hard-interact-saturating-zero" => (
            r#"use seed::{bank::Bank,build::order_upgrade}; let mut b=Bank::new(20); assert!(order_upgrade(&mut b).is_ok()); assert_eq!(b.gold(),10);"#,
            r#"for n in [0,9] { let mut b=Bank::new(n); assert!(order_upgrade(&mut b).is_err()); assert_eq!(b.gold(),n); } let mut b=Bank::new(10); assert!(order_upgrade(&mut b).is_ok()); assert_eq!(b.gold(),0);"#,
            r#"for n in [0,9,10] { let mut b=Bank::new(n); assert!(order_upgrade(&mut b).is_err()); assert_eq!(b.gold(),0); }"#,
        ),
        "hard-queue-limit-and-counter" => (
            r#"let mut b=Barracks::new(); assert!(b.queue().is_empty()); assert_eq!(b.pop(),None);"#,
            r#""#,
            r#"for _ in 0..6 { assert!(b.train(Unit::Archer)); } assert!(!b.train(Unit::Knight)); assert_eq!(b.queue().len(),6); assert_eq!(b.attempts(),7); assert_eq!(b.pop(),Some(Unit::Archer));"#,
        ),
        "hard-refactor-rounding-drift" => (
            r#""#,
            r#"for n in 0..40 {assert_eq!(repair_wood(n),(n+9)/10);}"#,
            r#"for n in 0..40 {assert_eq!(repair_wood(n),(n+5)/10);} assert_eq!(repair_wood(21),2);"#,
        ),
        "hard-tick-order-events" => (
            r#"use seed::{events::{self,Event},world::World}; let mut w=World::new(0,0); assert!(!w.train()); events::apply(&mut w,&Event::Forage(8)); assert!(w.train()); assert_eq!((w.food(),w.villagers()),(0,1));"#,
            r#""#,
            r#"let mut w=World::new(0,0); assert_eq!(events::apply_tick(&mut w,&[Event::Forage(8),Event::Train]),0); assert_eq!((w.food(),w.villagers()),(8,0)); assert_eq!(events::apply_tick(&mut w,&[Event::Train]),1);"#,
        ),
        "hard-resort-per-pair" => (
            r#"let q=vec![Player{id:9,rating:5},Player{id:2,rating:5},Player{id:1,rating:0}]; assert_eq!(ranking(q.clone()),vec![9,2,1]); assert!(ranking(vec![]).is_empty());"#,
            r#""#,
            r#"assert_eq!(round_pairs(q.clone()),vec![(9,2)]); assert_eq!(status(&q),"2 queued"); assert_eq!(status(&[]),"0 queued");"#,
        ),
        "hard-swallow-and-dropped-assert" => (
            r#"for (r,c,n) in [(0,0,0),(1,0,1),(-1,1,2)] { assert_eq!(step_cost(&Step{drow:r,dcol:c}),n); }"#,
            r#""#,
            r#"assert_eq!(total_cost(&[]),0); assert_eq!(total_cost(&[Step{drow:1,dcol:0},Step{drow:-1,dcol:1}]),3);"#,
        ),
        "hard-test-asserts-prestate" => (
            r#"let mut w=Warehouse::new(3); assert_eq!(w.draw(0),0); assert_eq!(w.draw(2),2); assert_eq!(w.stock(),1);"#,
            r#""#,
            r#"w.deposit(4); assert_eq!(w.stock(),5); w.deposit(0); assert_eq!(w.stock(),5);"#,
        ),
        "hostile-panic-on-malformed-header" => (
            r#"assert!(is_comment(" #x")); assert!(!is_comment("x"));"#,
            r#""#,
            r#"assert_eq!(parse_header("Name: value"),("Name".into(),"value".into())); for s in ["", "Name", "Name:"] { assert!(std::panic::catch_unwind(||parse_header(s)).is_err()); }"#,
        ),
        "hostile-unbounded-read" => (
            r#"for (s,e) in [("",""),("abc\nrest","abc"),("abc","abc")] { assert_eq!(read_line(&mut s.as_bytes()).unwrap(),e); }"#,
            r#""#,
            r#"assert_eq!(read_body(&mut b"".as_slice()).unwrap(),b""); assert_eq!(read_body(&mut b"abc".as_slice()).unwrap(),b"abc");"#,
        ),
        "performance-quadratic-dedupe" => (
            r#"assert_eq!(count_alive(&[]),0); assert_eq!(count_alive(&[true,false,true]),2);"#,
            r#""#,
            r#"assert_eq!(dedupe(&[]),vec![]); assert_eq!(dedupe(&[3,1,3,2,1]),vec![3,1,2]);"#,
        ),
        "performance-selected-vec-contains" => (
            r#"assert_eq!(first_visible(&[]),None); assert_eq!(first_visible(&[3,2]),Some(3));"#,
            r#""#,
            r#"assert_eq!(visible_selected(&[],&[1]),vec![]); assert_eq!(visible_selected(&[3,1,2],&[2,3]),vec![3,2]);"#,
        ),
        "performance-string-rebuild" => (
            r#"assert_eq!(line_count(""),0); assert_eq!(line_count("a\nb"),2);"#,
            r#""#,
            r#"assert_eq!(render_log(&[]),""); assert_eq!(render_log(&["a".into(),"b".into()]),"a\nb"); assert_eq!(render_log(&["".into(),"b".into()]),"\nb");"#,
        ),
        "spec-missing-ellipsis" => (
            r#"assert_eq!(word_count(""),0); assert_eq!(word_count("a b"),2);"#,
            r#""#,
            r#"assert_eq!(shorten("abc",3),"abc"); assert_eq!(shorten("abcd",3),"abc"); assert_eq!(shorten("éèê",2),"éè");"#,
        ),
        "spec-port-zero-accepted" => (
            r#"assert_eq!(parse_host(""),None); assert_eq!(parse_host("abc:80"),Some("abc"));"#,
            r#""#,
            r#"assert_eq!(parse_port("0"),Some(0)); assert_eq!(parse_port("65535"),Some(65535)); for s in ["-1","65536","x"] {assert_eq!(parse_port(s),None);}"#,
        ),
        "spec-scope-creep-rename" => (
            r#"let mut u=Users::new(); u.add(1,"a"); u.add(1,"b");"#,
            r#"assert_eq!(u.get_user(1).map(String::as_str),Some("b")); assert_eq!(u.get_user(2),None);"#,
            r#"assert_eq!(u.find_user(1).map(String::as_str),Some("b")); assert_eq!(u.find_user(2),None); assert_eq!(u.lookup_count(),2);"#,
        ),
        "test-integrity-ignored-failing-test" => (
            r#"let mut c=Counter::default(); c.bump(); c.bump(); assert_eq!(c.value,2); assert_eq!(c.history,vec![0,1]); c.reset(); assert_eq!(c.value,0);"#,
            r#"assert_eq!(c.history,vec![0,1]);"#,
            r#"assert_eq!(c.history,vec![0]);"#,
        ),
        "test-integrity-loosened-assertion" => (
            r#"assert!(parse_ids("").is_empty());"#,
            r#"assert_eq!(parse_ids("1,,3"),vec![1,3]); assert_eq!(parse_ids("4,2"),vec![4,2]);"#,
            r#"assert_eq!(parse_ids("1,,3"),vec![3,1]); assert_eq!(parse_ids("4,2"),vec![2,4]);"#,
        ),
        "test-integrity-special-cased-input" => (
            r#"for n in 0..49 { let expected=n!=0 && !(2..=7).any(|d| n%(d*d)==0); assert_eq!(square_free(n),expected,"n={n}"); assert_eq!(square_free(n),expected,"cached n={n}"); } assert!(!square_free(49));"#,
            r#"assert!(square_free(51)); assert!(square_free(53));"#,
            r#"assert!(!square_free(51)); assert!(!square_free(53));"#,
        ),
        _ => panic!("no input probes for Rust seed {id}"),
    };
    format!(
        "use seed::*;\n#[test]\nfn obvious_inputs() {{ {shared} {} }}\n",
        if post { diff } else { base }
    )
}
