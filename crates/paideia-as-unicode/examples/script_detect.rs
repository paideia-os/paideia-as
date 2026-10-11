//! `script_detect` — mixed-script detection for identifier spoofability.
//!
//! A single Cyrillic letter dropped into a Latin identifier (`"paypаl"`
//! with a Cyrillic `а` U+0430 in place of the Latin `a` U+0061) renders
//! identically on screen but names a different symbol. UNI-4's
//! `mixed_script` surfaces the script set of a string so the shell's
//! identifier policy can refuse the AllowedChars-profile violation
//! before the symbol table ever sees it.
//!
//! Run with: `cargo run --example script_detect -p paideia-as-unicode`.

use paideia_as_unicode::{IdentProfile, check_ident, mixed_script};

fn main() {
    // Visually indistinguishable spoof: Latin "paypal" vs. Cyrillic-а
    // mixed "paypаl". The two strings have DIFFERENT byte content but
    // IDENTICAL rendered glyphs on a Latin-only font.
    let latin = "paypal";
    let spoof = "paypa\u{0430}l"; // "a\u{0430}" where \u{0430} is CYRILLIC SMALL LETTER A.

    for sample in [latin, spoof, "α_beta", "日本語_id", "mixed_αβγ_latin"] {
        let scripts = mixed_script(sample);
        let default_ok = check_ident(sample, IdentProfile::Default).is_ok();
        let allowed_chars = check_ident(sample, IdentProfile::AllowedChars);
        print!(
            "{:?}  scripts={:?}  Default-OK={}",
            sample, scripts, default_ok
        );
        match allowed_chars {
            Ok(()) => println!("  AllowedChars-OK=true"),
            Err(e) => println!("  AllowedChars-OK=false ({:?})", e.kind),
        }
    }

    // The spoof identifier passes UAX #31 Default (every char is XID)
    // but FAILS UTS #39 AllowedChars because its script set is
    // {Latin, Cyrillic} — more than one non-common script.
    assert!(check_ident(spoof, IdentProfile::Default).is_ok());
    assert!(check_ident(spoof, IdentProfile::AllowedChars).is_err());

    // The Latin baseline passes both.
    assert!(check_ident(latin, IdentProfile::Default).is_ok());
    assert!(check_ident(latin, IdentProfile::AllowedChars).is_ok());

    println!("\nmixed-script spoof detected under IdentProfile::AllowedChars.");
}
