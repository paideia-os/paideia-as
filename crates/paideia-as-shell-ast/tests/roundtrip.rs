//! R221.M5/M7 round-trip fixtures: 25 individual `#[test]` fixtures
//! plus a bulk 100-sample sweep in `r221m7_roundtrip_bulk_100`.
//!
//! The round-trip contract: `pretty_print(parse(src))` must be
//! reparseable and pretty-print to byte-identical output. See
//! `common::assert_roundtrip`.

mod common;
use common::assert_roundtrip;

macro_rules! rt {
    ($name:ident, $fp:expr, $src:expr) => {
        #[test]
        fn $name() {
            assert_roundtrip($fp, $src);
        }
    };
}

// ---- Individual fixtures ---------------------------------------
rt!(r221m7_rt_01, "r221m7-rt-01", "ls");
rt!(r221m7_rt_02, "r221m7-rt-02", "ls foo");
rt!(r221m7_rt_03, "r221m7-rt-03", "ls foo bar");
rt!(r221m7_rt_04, "r221m7-rt-04", "ls | wc");
rt!(r221m7_rt_05, "r221m7-rt-05", "cat f | sort | uniq");
rt!(r221m7_rt_06, "r221m7-rt-06", "ls; wc");
rt!(r221m7_rt_07, "r221m7-rt-07", "head 5");
rt!(r221m7_rt_08, "r221m7-rt-08", r#"echo "hello""#);
rt!(r221m7_rt_09, "r221m7-rt-09", r#"echo "hello world""#);
rt!(r221m7_rt_10, "r221m7-rt-10", "sort by f.size");
rt!(r221m7_rt_11, "r221m7-rt-11", "datalog { p(a). }");
rt!(r221m7_rt_12, "r221m7-rt-12", "datalog { parent(alice, bob). }");
rt!(r221m7_rt_13, "r221m7-rt-13", "datalog { p(a). q(b). }");
rt!(r221m7_rt_14, "r221m7-rt-14", "datalog { p(?x) => q(?x). }");
rt!(
    r221m7_rt_15,
    "r221m7-rt-15",
    "datalog { anc(?x, ?z) => par(?x, ?y), anc(?y, ?z). }"
);
rt!(r221m7_rt_16, "r221m7-rt-16", "datalog { not banned(?x). }");
rt!(r221m7_rt_17, "r221m7-rt-17", "{ |x| x }");
rt!(r221m7_rt_18, "r221m7-rt-18", "{ |x| x + 1 }");
rt!(r221m7_rt_19, "r221m7-rt-19", "{ |x y| x + y }");
rt!(r221m7_rt_20, "r221m7-rt-20", "{ 42 }");
rt!(r221m7_rt_21, "r221m7-rt-21", "ls | filter { |f| f.size > 100 }");
rt!(
    r221m7_rt_22,
    "r221m7-rt-22",
    "files | datalog { tag(?f, \"red\"). }"
);
rt!(r221m7_rt_23, "r221m7-rt-23", "ls > out");
rt!(r221m7_rt_24, "r221m7-rt-24", "wc < in");
rt!(r221m7_rt_25, "r221m7-rt-25", "( ls | wc )");

// ---- 100-sample bulk sweep -------------------------------------

/// Programmatically synthesize 100 canonical inputs and round-trip each.
/// Fingerprint `r221m7-rt-bulk-<N>`.
#[test]
fn r221m7_rt_bulk_100() {
    let mut samples: Vec<String> = Vec::with_capacity(100);
    let names = [
        "ls", "cd", "grep", "cat", "head", "tail", "sort", "uniq",
        "wc", "awk", "sed", "cut", "tr", "date", "df", "du", "top",
        "ps", "id", "who",
    ];
    // 20 x bare command
    for name in names {
        samples.push(name.to_string());
    }
    // 20 x pipe pair
    for name in names {
        samples.push(format!("{name} | wc"));
    }
    // 20 x cmd+string-arg
    for name in names {
        samples.push(format!(r#"{name} "hello""#));
    }
    // 20 x datalog fact with pred=name
    for name in names {
        samples.push(format!("datalog {{ {name}(a, b). }}"));
    }
    // 20 x lambda { |x| x + i }
    for (i, _) in names.iter().enumerate() {
        samples.push(format!("{{ |x| x + {i} }}"));
    }
    assert_eq!(samples.len(), 100, "sample corpus must be exactly 100");
    for (i, s) in samples.iter().enumerate() {
        assert_roundtrip(&format!("r221m7-rt-bulk-{i:03}"), s);
    }
}
