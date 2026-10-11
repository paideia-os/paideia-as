//! UNI-5 (paideia-as#1592) — UCD property adapter: General_Category /
//! Script / Block.
//!
//! # Why this crate owns three lookups
//!
//! The semantic-shell's higher layers (identifier validation at UNI-4,
//! segmentation at UNI-3, security checks per UTS #39) all speak the UCD
//! property vocabulary. Rather than let every consumer wire its own
//! property-table dependency, UNI-5 collects the three canonical
//! `uax44` properties — General_Category, Script, Block — behind a
//! single `ucd` surface.
//!
//! # Dependency choice
//!
//! Three well-maintained Servo/`unicode-rs` crates in the same vendor
//! family as this crate's existing `unicode-normalization` /
//! `unicode-segmentation` / `unicode-width` deps:
//!
//! * `unicode-properties` 0.1 — UAX #44 General_Category. Supplies 30
//!   long-form variants (`UppercaseLetter`, `LowercaseLetter`, …). We
//!   expose UCD short-form variants (`Lu`, `Ll`, …) via our own
//!   [`GeneralCategory`] enum and translate at the boundary.
//!
//! * `unicode-script` 0.5 — UAX #24 Script. Already
//!   `#[non_exhaustive]` and already matches our
//!   `#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]` shape, so we
//!   `pub use` the upstream enum as [`Script`]. Includes the three
//!   out-of-band values UAX #24 requires: `Common` (shared across
//!   scripts, e.g. ASCII punctuation), `Inherited` (combining marks
//!   that take the base character's script), `Unknown` (unassigned).
//!
//! * `unicode-blocks` 0.1 — UCD `Blocks.txt`. Upstream exposes 344
//!   `UnicodeBlock` struct constants rather than an enum, so we
//!   hand-roll a 344-variant [`Block`] enum keyed off
//!   `UnicodeBlock::start()` (which uniquely identifies a block within
//!   U+0000..=U+10FFFF). The dispatch map's §UNI-5 Block-fallback
//!   rationale: once UNI-7 vendors `data/UCD/Blocks.txt`, this match
//!   table becomes a candidate for codegen regeneration.
//!
//! # Why `#[non_exhaustive]`
//!
//! Every new UCD release can add new General_Category / Script / Block
//! variants (Unicode 15.1 landed Nag Mundari; 16.0 landed Garay;
//! unicode-blocks is on 17.0.0). Pinning the enum shape to the current
//! UCD revision would force a crate-wide major-version bump at every
//! UCD cadence — `#[non_exhaustive]` keeps consumer `match` blocks
//! sound across minor UCD bumps.
//!
//! # Convenience predicates
//!
//! Seven group predicates (`is_letter`, `is_mark`, `is_number`,
//! `is_punctuation`, `is_symbol`, `is_separator`, `is_other`) collapse
//! the 30 General_Category variants to the seven top-level groups the
//! Unicode Standard §4.5 enumerates. Each is a thin wrapper around a
//! match on [`general_category`].
//!
//! # Invariants
//!
//! For every `c: char`:
//! * Exactly one of the seven group predicates returns true.
//! * `block(c)` returns `Block::Unassigned` iff the codepoint lies in a
//!   gap between assigned blocks (Blocks.txt coverage is not
//!   contiguous).
//! * `script(c)` returns `Script::Unknown` for unassigned codepoints.
//!
//! See `design/toolchain/paideia-as-unicode-spec.md` §4.9 for the
//! full API-surface contract.

use unicode_properties::GeneralCategory as UpGc;
use unicode_properties::UnicodeGeneralCategory;
use unicode_script::UnicodeScript;

// Re-export the upstream `unicode-script` Script enum as our own public
// `Script` type. See the module header for the rationale.
pub use unicode_script::Script;

/// UCD General_Category property per UAX #44 §5.7.
///
/// Variant names follow the UCD two-letter short form (`Lu`, `Ll`,
/// `Lt`, …) rather than the long form `unicode-properties` uses
/// internally. Short form matches the Unicode Standard's own text and
/// keeps call-sites compact (`gc == Lu`, not
/// `gc == UppercaseLetter`).
///
/// `#[non_exhaustive]`: future UCD revisions may add variants.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum GeneralCategory {
    /// `Lu` — uppercase letter.
    Lu,
    /// `Ll` — lowercase letter.
    Ll,
    /// `Lt` — titlecase letter (digraphic, first part uppercase).
    Lt,
    /// `Lm` — modifier letter.
    Lm,
    /// `Lo` — other letter (syllables, ideographs).
    Lo,
    /// `Mn` — nonspacing combining mark (zero advance).
    Mn,
    /// `Mc` — spacing combining mark.
    Mc,
    /// `Me` — enclosing combining mark.
    Me,
    /// `Nd` — decimal digit.
    Nd,
    /// `Nl` — letterlike numeric character.
    Nl,
    /// `No` — other numeric character.
    No,
    /// `Pc` — connector punctuation (ties).
    Pc,
    /// `Pd` — dash or hyphen punctuation.
    Pd,
    /// `Ps` — opening paired punctuation.
    Ps,
    /// `Pe` — closing paired punctuation.
    Pe,
    /// `Pi` — initial quotation mark.
    Pi,
    /// `Pf` — final quotation mark.
    Pf,
    /// `Po` — other punctuation.
    Po,
    /// `Sm` — mathematical symbol.
    Sm,
    /// `Sc` — currency symbol.
    Sc,
    /// `Sk` — non-letterlike modifier symbol.
    Sk,
    /// `So` — other symbol.
    So,
    /// `Zs` — space separator.
    Zs,
    /// `Zl` — line separator (U+2028 only).
    Zl,
    /// `Zp` — paragraph separator (U+2029 only).
    Zp,
    /// `Cc` — C0 or C1 control code.
    Cc,
    /// `Cf` — format control character.
    Cf,
    /// `Cs` — surrogate code point.
    Cs,
    /// `Co` — private-use character.
    Co,
    /// `Cn` — reserved unassigned or noncharacter.
    Cn,
}

impl From<UpGc> for GeneralCategory {
    fn from(c: UpGc) -> Self {
        match c {
            UpGc::UppercaseLetter => GeneralCategory::Lu,
            UpGc::LowercaseLetter => GeneralCategory::Ll,
            UpGc::TitlecaseLetter => GeneralCategory::Lt,
            UpGc::ModifierLetter => GeneralCategory::Lm,
            UpGc::OtherLetter => GeneralCategory::Lo,
            UpGc::NonspacingMark => GeneralCategory::Mn,
            UpGc::SpacingMark => GeneralCategory::Mc,
            UpGc::EnclosingMark => GeneralCategory::Me,
            UpGc::DecimalNumber => GeneralCategory::Nd,
            UpGc::LetterNumber => GeneralCategory::Nl,
            UpGc::OtherNumber => GeneralCategory::No,
            UpGc::ConnectorPunctuation => GeneralCategory::Pc,
            UpGc::DashPunctuation => GeneralCategory::Pd,
            UpGc::OpenPunctuation => GeneralCategory::Ps,
            UpGc::ClosePunctuation => GeneralCategory::Pe,
            UpGc::InitialPunctuation => GeneralCategory::Pi,
            UpGc::FinalPunctuation => GeneralCategory::Pf,
            UpGc::OtherPunctuation => GeneralCategory::Po,
            UpGc::MathSymbol => GeneralCategory::Sm,
            UpGc::CurrencySymbol => GeneralCategory::Sc,
            UpGc::ModifierSymbol => GeneralCategory::Sk,
            UpGc::OtherSymbol => GeneralCategory::So,
            UpGc::SpaceSeparator => GeneralCategory::Zs,
            UpGc::LineSeparator => GeneralCategory::Zl,
            UpGc::ParagraphSeparator => GeneralCategory::Zp,
            UpGc::Control => GeneralCategory::Cc,
            UpGc::Format => GeneralCategory::Cf,
            UpGc::Surrogate => GeneralCategory::Cs,
            UpGc::PrivateUse => GeneralCategory::Co,
            UpGc::Unassigned => GeneralCategory::Cn,
        }
    }
}

/// Query the UAX #44 General_Category of a character.
///
/// Table-driven lookup via `unicode-properties`. Constant time per
/// character (bitpack + skip-table).
///
/// # Examples
///
/// ```
/// use paideia_as_unicode::{general_category, GeneralCategory};
/// assert_eq!(general_category('A'), GeneralCategory::Lu);
/// assert_eq!(general_category('a'), GeneralCategory::Ll);
/// assert_eq!(general_category('5'), GeneralCategory::Nd);
/// ```
pub fn general_category(c: char) -> GeneralCategory {
    c.general_category().into()
}

/// Query the UAX #24 Script of a character.
///
/// `Script::Common` is returned for script-neutral codepoints (ASCII
/// punctuation, CJK symbols, etc.); `Script::Inherited` for combining
/// marks that take the base character's script; `Script::Unknown` for
/// unassigned codepoints.
///
/// # Examples
///
/// ```
/// use paideia_as_unicode::{script, Script};
/// assert_eq!(script('a'), Script::Latin);
/// assert_eq!(script('α'), Script::Greek);
/// ```
pub fn script(c: char) -> Script {
    c.script()
}

/// Query the UCD Blocks.txt block of a character.
///
/// Returns [`Block::Unassigned`] for codepoints in the gaps between
/// assigned blocks (Blocks.txt coverage is not contiguous — U+0870..
/// is in Arabic Extended-B, U+0880.. is a gap, etc.).
///
/// # Examples
///
/// ```
/// use paideia_as_unicode::{block, Block};
/// assert_eq!(block('a'), Block::BasicLatin);
/// assert_eq!(block('à'), Block::Latin1Supplement);
/// ```
pub fn block(c: char) -> Block {
    match unicode_blocks::find_unicode_block(c) {
        Some(b) => block_from_start(b.start()),
        None => Block::Unassigned,
    }
}

/// Translate a `unicode_blocks::UnicodeBlock::start()` codepoint into
/// our local [`Block`] enum.
///
/// Every start value is unique across the 344 blocks (two blocks
/// never share a start codepoint). An unmatched start falls through
/// to `Block::Unassigned`, which is unreachable in practice — the
/// upstream crate's `find_unicode_block` only returns blocks whose
/// start we enumerate here — but defends against upstream additions
/// before this table is regenerated.
fn block_from_start(start: u32) -> Block {
    match start {
        0 => Block::BasicLatin,
        128 => Block::Latin1Supplement,
        256 => Block::LatinExtendedA,
        384 => Block::LatinExtendedB,
        592 => Block::IpaExtensions,
        688 => Block::SpacingModifierLetters,
        768 => Block::CombiningDiacriticalMarks,
        880 => Block::GreekAndCoptic,
        1024 => Block::Cyrillic,
        1280 => Block::CyrillicSupplement,
        1328 => Block::Armenian,
        1424 => Block::Hebrew,
        1536 => Block::Arabic,
        1792 => Block::Syriac,
        1872 => Block::ArabicSupplement,
        1920 => Block::Thaana,
        1984 => Block::Nko,
        2048 => Block::Samaritan,
        2112 => Block::Mandaic,
        2144 => Block::SyriacSupplement,
        2160 => Block::ArabicExtendedB,
        2208 => Block::ArabicExtendedA,
        2304 => Block::Devanagari,
        2432 => Block::Bengali,
        2560 => Block::Gurmukhi,
        2688 => Block::Gujarati,
        2816 => Block::Oriya,
        2944 => Block::Tamil,
        3072 => Block::Telugu,
        3200 => Block::Kannada,
        3328 => Block::Malayalam,
        3456 => Block::Sinhala,
        3584 => Block::Thai,
        3712 => Block::Lao,
        3840 => Block::Tibetan,
        4096 => Block::Myanmar,
        4256 => Block::Georgian,
        4352 => Block::HangulJamo,
        4608 => Block::Ethiopic,
        4992 => Block::EthiopicSupplement,
        5024 => Block::Cherokee,
        5120 => Block::UnifiedCanadianAboriginalSyllabics,
        5760 => Block::Ogham,
        5792 => Block::Runic,
        5888 => Block::Tagalog,
        5920 => Block::Hanunoo,
        5952 => Block::Buhid,
        5984 => Block::Tagbanwa,
        6016 => Block::Khmer,
        6144 => Block::Mongolian,
        6400 => Block::Limbu,
        6480 => Block::TaiLe,
        6528 => Block::NewTaiLue,
        6624 => Block::KhmerSymbols,
        6656 => Block::Buginese,
        6688 => Block::TaiTham,
        6832 => Block::CombiningDiacriticalMarksExtended,
        6912 => Block::Balinese,
        7040 => Block::Sundanese,
        7104 => Block::Batak,
        7168 => Block::Lepcha,
        7248 => Block::OlChiki,
        7296 => Block::CyrillicExtendedC,
        7312 => Block::GeorgianExtended,
        7360 => Block::SundaneseSupplement,
        7376 => Block::VedicExtensions,
        7424 => Block::PhoneticExtensions,
        7552 => Block::PhoneticExtensionsSupplement,
        7616 => Block::CombiningDiacriticalMarksSupplement,
        7680 => Block::LatinExtendedAdditional,
        7936 => Block::GreekExtended,
        8192 => Block::GeneralPunctuation,
        8304 => Block::SuperscriptsAndSubscripts,
        8352 => Block::CurrencySymbols,
        8400 => Block::CombiningDiacriticalMarksForSymbols,
        8448 => Block::LetterlikeSymbols,
        8528 => Block::NumberForms,
        8592 => Block::Arrows,
        8704 => Block::MathematicalOperators,
        8960 => Block::MiscellaneousTechnical,
        9216 => Block::ControlPictures,
        9280 => Block::OpticalCharacterRecognition,
        9312 => Block::EnclosedAlphanumerics,
        9472 => Block::BoxDrawing,
        9600 => Block::BlockElements,
        9632 => Block::GeometricShapes,
        9728 => Block::MiscellaneousSymbols,
        9984 => Block::Dingbats,
        10176 => Block::MiscellaneousMathematicalSymbolsA,
        10224 => Block::SupplementalArrowsA,
        10240 => Block::BraillePatterns,
        10496 => Block::SupplementalArrowsB,
        10624 => Block::MiscellaneousMathematicalSymbolsB,
        10752 => Block::SupplementalMathematicalOperators,
        11008 => Block::MiscellaneousSymbolsAndArrows,
        11264 => Block::Glagolitic,
        11360 => Block::LatinExtendedC,
        11392 => Block::Coptic,
        11520 => Block::GeorgianSupplement,
        11568 => Block::Tifinagh,
        11648 => Block::EthiopicExtended,
        11744 => Block::CyrillicExtendedA,
        11776 => Block::SupplementalPunctuation,
        11904 => Block::CjkRadicalsSupplement,
        12032 => Block::KangxiRadicals,
        12272 => Block::IdeographicDescriptionCharacters,
        12288 => Block::CjkSymbolsAndPunctuation,
        12352 => Block::Hiragana,
        12448 => Block::Katakana,
        12544 => Block::Bopomofo,
        12592 => Block::HangulCompatibilityJamo,
        12688 => Block::Kanbun,
        12704 => Block::BopomofoExtended,
        12736 => Block::CjkStrokes,
        12784 => Block::KatakanaPhoneticExtensions,
        12800 => Block::EnclosedCjkLettersAndMonths,
        13056 => Block::CjkCompatibility,
        13312 => Block::CjkUnifiedIdeographsExtensionA,
        19904 => Block::YijingHexagramSymbols,
        19968 => Block::CjkUnifiedIdeographs,
        40960 => Block::YiSyllables,
        42128 => Block::YiRadicals,
        42192 => Block::Lisu,
        42240 => Block::Vai,
        42560 => Block::CyrillicExtendedB,
        42656 => Block::Bamum,
        42752 => Block::ModifierToneLetters,
        42784 => Block::LatinExtendedD,
        43008 => Block::SylotiNagri,
        43056 => Block::CommonIndicNumberForms,
        43072 => Block::PhagsPa,
        43136 => Block::Saurashtra,
        43232 => Block::DevanagariExtended,
        43264 => Block::KayahLi,
        43312 => Block::Rejang,
        43360 => Block::HangulJamoExtendedA,
        43392 => Block::Javanese,
        43488 => Block::MyanmarExtendedB,
        43520 => Block::Cham,
        43616 => Block::MyanmarExtendedA,
        43648 => Block::TaiViet,
        43744 => Block::MeeteiMayekExtensions,
        43776 => Block::EthiopicExtendedA,
        43824 => Block::LatinExtendedE,
        43888 => Block::CherokeeSupplement,
        43968 => Block::MeeteiMayek,
        44032 => Block::HangulSyllables,
        55216 => Block::HangulJamoExtendedB,
        55296 => Block::HighSurrogates,
        56192 => Block::HighPrivateUseSurrogates,
        56320 => Block::LowSurrogates,
        57344 => Block::PrivateUseArea,
        63744 => Block::CjkCompatibilityIdeographs,
        64256 => Block::AlphabeticPresentationForms,
        64336 => Block::ArabicPresentationFormsA,
        65024 => Block::VariationSelectors,
        65040 => Block::VerticalForms,
        65056 => Block::CombiningHalfMarks,
        65072 => Block::CjkCompatibilityForms,
        65104 => Block::SmallFormVariants,
        65136 => Block::ArabicPresentationFormsB,
        65280 => Block::HalfwidthAndFullwidthForms,
        65520 => Block::Specials,
        65536 => Block::LinearBSyllabary,
        65664 => Block::LinearBIdeograms,
        65792 => Block::AegeanNumbers,
        65856 => Block::AncientGreekNumbers,
        65936 => Block::AncientSymbols,
        66000 => Block::PhaistosDisc,
        66176 => Block::Lycian,
        66208 => Block::Carian,
        66272 => Block::CopticEpactNumbers,
        66304 => Block::OldItalic,
        66352 => Block::Gothic,
        66384 => Block::OldPermic,
        66432 => Block::Ugaritic,
        66464 => Block::OldPersian,
        66560 => Block::Deseret,
        66640 => Block::Shavian,
        66688 => Block::Osmanya,
        66736 => Block::Osage,
        66816 => Block::Elbasan,
        66864 => Block::CaucasianAlbanian,
        66928 => Block::Vithkuqi,
        67008 => Block::Todhri,
        67072 => Block::LinearA,
        67456 => Block::LatinExtendedF,
        67584 => Block::CypriotSyllabary,
        67648 => Block::ImperialAramaic,
        67680 => Block::Palmyrene,
        67712 => Block::Nabataean,
        67808 => Block::Hatran,
        67840 => Block::Phoenician,
        67872 => Block::Lydian,
        67904 => Block::Sidetic,
        67968 => Block::MeroiticHieroglyphs,
        68000 => Block::MeroiticCursive,
        68096 => Block::Kharoshthi,
        68192 => Block::OldSouthArabian,
        68224 => Block::OldNorthArabian,
        68288 => Block::Manichaean,
        68352 => Block::Avestan,
        68416 => Block::InscriptionalParthian,
        68448 => Block::InscriptionalPahlavi,
        68480 => Block::PsalterPahlavi,
        68608 => Block::OldTurkic,
        68736 => Block::OldHungarian,
        68864 => Block::HanifiRohingya,
        68928 => Block::Garay,
        69216 => Block::RumiNumeralSymbols,
        69248 => Block::Yezidi,
        69312 => Block::ArabicExtendedC,
        69376 => Block::OldSogdian,
        69424 => Block::Sogdian,
        69488 => Block::OldUyghur,
        69552 => Block::Chorasmian,
        69600 => Block::Elymaic,
        69632 => Block::Brahmi,
        69760 => Block::Kaithi,
        69840 => Block::SoraSompeng,
        69888 => Block::Chakma,
        69968 => Block::Mahajani,
        70016 => Block::Sharada,
        70112 => Block::SinhalaArchaicNumbers,
        70144 => Block::Khojki,
        70272 => Block::Multani,
        70320 => Block::Khudawadi,
        70400 => Block::Grantha,
        70528 => Block::TuluTigalari,
        70656 => Block::Newa,
        70784 => Block::Tirhuta,
        71040 => Block::Siddham,
        71168 => Block::Modi,
        71264 => Block::MongolianSupplement,
        71296 => Block::Takri,
        71376 => Block::MyanmarExtendedC,
        71424 => Block::Ahom,
        71680 => Block::Dogra,
        71840 => Block::WarangCiti,
        71936 => Block::DivesAkuru,
        72096 => Block::Nandinagari,
        72192 => Block::ZanabazarSquare,
        72272 => Block::Soyombo,
        72384 => Block::PauCinHau,
        72448 => Block::DevanagariExtendedA,
        72544 => Block::SharadaSupplement,
        72640 => Block::Sunuwar,
        72704 => Block::Bhaiksuki,
        72816 => Block::Marchen,
        72960 => Block::MasaramGondi,
        73056 => Block::GunjalaGondi,
        73136 => Block::TolongSiki,
        73440 => Block::Makasar,
        73472 => Block::Kawi,
        73648 => Block::LisuSupplement,
        73664 => Block::TamilSupplement,
        73728 => Block::Cuneiform,
        74752 => Block::CuneiformNumbersAndPunctuation,
        74880 => Block::EarlyDynasticCuneiform,
        77712 => Block::CyproMinoan,
        77824 => Block::EgyptianHieroglyphs,
        78896 => Block::EgyptianHieroglyphFormatControls,
        78944 => Block::EgyptianHieroglyphsExtendedA,
        82944 => Block::AnatolianHieroglyphs,
        90368 => Block::GurungKhema,
        92160 => Block::BamumSupplement,
        92736 => Block::Mro,
        92784 => Block::Tangsa,
        92880 => Block::BassaVah,
        92928 => Block::PahawhHmong,
        93504 => Block::KiratRai,
        93760 => Block::Medefaidrin,
        93856 => Block::BeriaErfe,
        93952 => Block::Miao,
        94176 => Block::IdeographicSymbolsAndPunctuation,
        94208 => Block::Tangut,
        100352 => Block::TangutComponents,
        101120 => Block::KhitanSmallScript,
        101632 => Block::TangutSupplement,
        101760 => Block::TangutComponentsSupplement,
        110576 => Block::KanaExtendedB,
        110592 => Block::KanaSupplement,
        110848 => Block::KanaExtendedA,
        110896 => Block::SmallKanaExtension,
        110960 => Block::Nushu,
        113664 => Block::Duployan,
        113824 => Block::ShorthandFormatControls,
        117760 => Block::SymbolsForLegacyComputingSupplement,
        118464 => Block::MiscellaneousSymbolsSupplement,
        118528 => Block::ZnamennyMusicalNotation,
        118784 => Block::ByzantineMusicalSymbols,
        119040 => Block::MusicalSymbols,
        119296 => Block::AncientGreekMusicalNotation,
        119488 => Block::KaktovikNumerals,
        119520 => Block::MayanNumerals,
        119552 => Block::TaiXuanJingSymbols,
        119648 => Block::CountingRodNumerals,
        119808 => Block::MathematicalAlphanumericSymbols,
        120832 => Block::SuttonSignwriting,
        122624 => Block::LatinExtendedG,
        122880 => Block::GlagoliticSupplement,
        122928 => Block::CyrillicExtendedD,
        123136 => Block::NyiakengPuachueHmong,
        123536 => Block::Toto,
        123584 => Block::Wancho,
        124112 => Block::NagMundari,
        124368 => Block::OlOnal,
        124608 => Block::TaiYo,
        124896 => Block::EthiopicExtendedB,
        124928 => Block::MendeKikakui,
        125184 => Block::Adlam,
        126064 => Block::IndicSiyaqNumbers,
        126208 => Block::OttomanSiyaqNumbers,
        126464 => Block::ArabicMathematicalAlphabeticSymbols,
        126976 => Block::MahjongTiles,
        127024 => Block::DominoTiles,
        127136 => Block::PlayingCards,
        127232 => Block::EnclosedAlphanumericSupplement,
        127488 => Block::EnclosedIdeographicSupplement,
        127744 => Block::MiscellaneousSymbolsAndPictographs,
        128512 => Block::Emoticons,
        128592 => Block::OrnamentalDingbats,
        128640 => Block::TransportAndMapSymbols,
        128768 => Block::AlchemicalSymbols,
        128896 => Block::GeometricShapesExtended,
        129024 => Block::SupplementalArrowsC,
        129280 => Block::SupplementalSymbolsAndPictographs,
        129536 => Block::ChessSymbols,
        129648 => Block::SymbolsAndPictographsExtendedA,
        129792 => Block::SymbolsForLegacyComputing,
        131072 => Block::CjkUnifiedIdeographsExtensionB,
        173824 => Block::CjkUnifiedIdeographsExtensionC,
        177984 => Block::CjkUnifiedIdeographsExtensionD,
        178208 => Block::CjkUnifiedIdeographsExtensionE,
        183984 => Block::CjkUnifiedIdeographsExtensionF,
        191472 => Block::CjkUnifiedIdeographsExtensionI,
        194560 => Block::CjkCompatibilityIdeographsSupplement,
        196608 => Block::CjkUnifiedIdeographsExtensionG,
        201552 => Block::CjkUnifiedIdeographsExtensionH,
        205744 => Block::CjkUnifiedIdeographsExtensionJ,
        917504 => Block::Tags,
        917760 => Block::VariationSelectorsSupplement,
        983040 => Block::SupplementaryPrivateUseAreaA,
        1048576 => Block::SupplementaryPrivateUseAreaB,
        _ => Block::Unassigned,
    }
}

/// UCD Blocks.txt block.
///
/// Variant names follow the Unicode Standard's own block names in
/// `CamelCase` (`BasicLatin`, `Latin1Supplement`, `GreekAndCoptic`,
/// …), consistent with the UCS character-chart titles. 344 named
/// blocks per UCD 17.0 (unicode-blocks 0.1.10) plus `Unassigned` for
/// gap codepoints.
///
/// `#[non_exhaustive]`: every UCD release adds blocks.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Block {
    /// `Basic Latin` (U+0000..=U+007F).
    BasicLatin,
    /// `Latin-1 Supplement` (U+0080..=U+00FF).
    Latin1Supplement,
    /// `Latin Extended-A` (U+0100..=U+017F).
    LatinExtendedA,
    /// `Latin Extended-B` (U+0180..=U+024F).
    LatinExtendedB,
    /// `IPA Extensions` (U+0250..=U+02AF).
    IpaExtensions,
    /// `Spacing Modifier Letters` (U+02B0..=U+02FF).
    SpacingModifierLetters,
    /// `Combining Diacritical Marks` (U+0300..=U+036F).
    CombiningDiacriticalMarks,
    /// `Greek and Coptic` (U+0370..=U+03FF).
    GreekAndCoptic,
    /// `Cyrillic` (U+0400..=U+04FF).
    Cyrillic,
    /// `Cyrillic Supplement` (U+0500..=U+052F).
    CyrillicSupplement,
    /// `Armenian` (U+0530..=U+058F).
    Armenian,
    /// `Hebrew` (U+0590..=U+05FF).
    Hebrew,
    /// `Arabic` (U+0600..=U+06FF).
    Arabic,
    /// `Syriac` (U+0700..=U+074F).
    Syriac,
    /// `Arabic Supplement` (U+0750..=U+077F).
    ArabicSupplement,
    /// `Thaana` (U+0780..=U+07BF).
    Thaana,
    /// `NKo` (U+07C0..=U+07FF).
    Nko,
    /// `Samaritan` (U+0800..=U+083F).
    Samaritan,
    /// `Mandaic` (U+0840..=U+085F).
    Mandaic,
    /// `Syriac Supplement` (U+0860..=U+086F).
    SyriacSupplement,
    /// `Arabic Extended-B` (U+0870..=U+089F).
    ArabicExtendedB,
    /// `Arabic Extended-A` (U+08A0..=U+08FF).
    ArabicExtendedA,
    /// `Devanagari` (U+0900..=U+097F).
    Devanagari,
    /// `Bengali` (U+0980..=U+09FF).
    Bengali,
    /// `Gurmukhi` (U+0A00..=U+0A7F).
    Gurmukhi,
    /// `Gujarati` (U+0A80..=U+0AFF).
    Gujarati,
    /// `Oriya` (U+0B00..=U+0B7F).
    Oriya,
    /// `Tamil` (U+0B80..=U+0BFF).
    Tamil,
    /// `Telugu` (U+0C00..=U+0C7F).
    Telugu,
    /// `Kannada` (U+0C80..=U+0CFF).
    Kannada,
    /// `Malayalam` (U+0D00..=U+0D7F).
    Malayalam,
    /// `Sinhala` (U+0D80..=U+0DFF).
    Sinhala,
    /// `Thai` (U+0E00..=U+0E7F).
    Thai,
    /// `Lao` (U+0E80..=U+0EFF).
    Lao,
    /// `Tibetan` (U+0F00..=U+0FFF).
    Tibetan,
    /// `Myanmar` (U+1000..=U+109F).
    Myanmar,
    /// `Georgian` (U+10A0..=U+10FF).
    Georgian,
    /// `Hangul Jamo` (U+1100..=U+11FF).
    HangulJamo,
    /// `Ethiopic` (U+1200..=U+137F).
    Ethiopic,
    /// `Ethiopic Supplement` (U+1380..=U+139F).
    EthiopicSupplement,
    /// `Cherokee` (U+13A0..=U+13FF).
    Cherokee,
    /// `Unified Canadian Aboriginal Syllabics` (U+1400..=U+167F).
    UnifiedCanadianAboriginalSyllabics,
    /// `Ogham` (U+1680..=U+169F).
    Ogham,
    /// `Runic` (U+16A0..=U+16FF).
    Runic,
    /// `Tagalog` (U+1700..=U+171F).
    Tagalog,
    /// `Hanunoo` (U+1720..=U+173F).
    Hanunoo,
    /// `Buhid` (U+1740..=U+175F).
    Buhid,
    /// `Tagbanwa` (U+1760..=U+177F).
    Tagbanwa,
    /// `Khmer` (U+1780..=U+17FF).
    Khmer,
    /// `Mongolian` (U+1800..=U+18AF).
    Mongolian,
    /// `Limbu` (U+1900..=U+194F).
    Limbu,
    /// `Tai Le` (U+1950..=U+197F).
    TaiLe,
    /// `New Tai Lue` (U+1980..=U+19DF).
    NewTaiLue,
    /// `Khmer Symbols` (U+19E0..=U+19FF).
    KhmerSymbols,
    /// `Buginese` (U+1A00..=U+1A1F).
    Buginese,
    /// `Tai Tham` (U+1A20..=U+1AAF).
    TaiTham,
    /// `Combining Diacritical Marks Extended` (U+1AB0..=U+1AFF).
    CombiningDiacriticalMarksExtended,
    /// `Balinese` (U+1B00..=U+1B7F).
    Balinese,
    /// `Sundanese` (U+1B80..=U+1BBF).
    Sundanese,
    /// `Batak` (U+1BC0..=U+1BFF).
    Batak,
    /// `Lepcha` (U+1C00..=U+1C4F).
    Lepcha,
    /// `Ol Chiki` (U+1C50..=U+1C7F).
    OlChiki,
    /// `Cyrillic Extended-C` (U+1C80..=U+1C8F).
    CyrillicExtendedC,
    /// `Georgian Extended` (U+1C90..=U+1CBF).
    GeorgianExtended,
    /// `Sundanese Supplement` (U+1CC0..=U+1CCF).
    SundaneseSupplement,
    /// `Vedic Extensions` (U+1CD0..=U+1CFF).
    VedicExtensions,
    /// `Phonetic Extensions` (U+1D00..=U+1D7F).
    PhoneticExtensions,
    /// `Phonetic Extensions Supplement` (U+1D80..=U+1DBF).
    PhoneticExtensionsSupplement,
    /// `Combining Diacritical Marks Supplement` (U+1DC0..=U+1DFF).
    CombiningDiacriticalMarksSupplement,
    /// `Latin Extended Additional` (U+1E00..=U+1EFF).
    LatinExtendedAdditional,
    /// `Greek Extended` (U+1F00..=U+1FFF).
    GreekExtended,
    /// `General Punctuation` (U+2000..=U+206F).
    GeneralPunctuation,
    /// `Superscripts and Subscripts` (U+2070..=U+209F).
    SuperscriptsAndSubscripts,
    /// `Currency Symbols` (U+20A0..=U+20CF).
    CurrencySymbols,
    /// `Combining Diacritical Marks for Symbols` (U+20D0..=U+20FF).
    CombiningDiacriticalMarksForSymbols,
    /// `Letterlike Symbols` (U+2100..=U+214F).
    LetterlikeSymbols,
    /// `Number Forms` (U+2150..=U+218F).
    NumberForms,
    /// `Arrows` (U+2190..=U+21FF).
    Arrows,
    /// `Mathematical Operators` (U+2200..=U+22FF).
    MathematicalOperators,
    /// `Miscellaneous Technical` (U+2300..=U+23FF).
    MiscellaneousTechnical,
    /// `Control Pictures` (U+2400..=U+243F).
    ControlPictures,
    /// `Optical Character Recognition` (U+2440..=U+245F).
    OpticalCharacterRecognition,
    /// `Enclosed Alphanumerics` (U+2460..=U+24FF).
    EnclosedAlphanumerics,
    /// `Box Drawing` (U+2500..=U+257F).
    BoxDrawing,
    /// `Block Elements` (U+2580..=U+259F).
    BlockElements,
    /// `Geometric Shapes` (U+25A0..=U+25FF).
    GeometricShapes,
    /// `Miscellaneous Symbols` (U+2600..=U+26FF).
    MiscellaneousSymbols,
    /// `Dingbats` (U+2700..=U+27BF).
    Dingbats,
    /// `Miscellaneous Mathematical Symbols-A` (U+27C0..=U+27EF).
    MiscellaneousMathematicalSymbolsA,
    /// `Supplemental Arrows-A` (U+27F0..=U+27FF).
    SupplementalArrowsA,
    /// `Braille Patterns` (U+2800..=U+28FF).
    BraillePatterns,
    /// `Supplemental Arrows-B` (U+2900..=U+297F).
    SupplementalArrowsB,
    /// `Miscellaneous Mathematical Symbols-B` (U+2980..=U+29FF).
    MiscellaneousMathematicalSymbolsB,
    /// `Supplemental Mathematical Operators` (U+2A00..=U+2AFF).
    SupplementalMathematicalOperators,
    /// `Miscellaneous Symbols and Arrows` (U+2B00..=U+2BFF).
    MiscellaneousSymbolsAndArrows,
    /// `Glagolitic` (U+2C00..=U+2C5F).
    Glagolitic,
    /// `Latin Extended-C` (U+2C60..=U+2C7F).
    LatinExtendedC,
    /// `Coptic` (U+2C80..=U+2CFF).
    Coptic,
    /// `Georgian Supplement` (U+2D00..=U+2D2F).
    GeorgianSupplement,
    /// `Tifinagh` (U+2D30..=U+2D7F).
    Tifinagh,
    /// `Ethiopic Extended` (U+2D80..=U+2DDF).
    EthiopicExtended,
    /// `Cyrillic Extended-A` (U+2DE0..=U+2DFF).
    CyrillicExtendedA,
    /// `Supplemental Punctuation` (U+2E00..=U+2E7F).
    SupplementalPunctuation,
    /// `CJK Radicals Supplement` (U+2E80..=U+2EFF).
    CjkRadicalsSupplement,
    /// `Kangxi Radicals` (U+2F00..=U+2FDF).
    KangxiRadicals,
    /// `Ideographic Description Characters` (U+2FF0..=U+2FFF).
    IdeographicDescriptionCharacters,
    /// `CJK Symbols and Punctuation` (U+3000..=U+303F).
    CjkSymbolsAndPunctuation,
    /// `Hiragana` (U+3040..=U+309F).
    Hiragana,
    /// `Katakana` (U+30A0..=U+30FF).
    Katakana,
    /// `Bopomofo` (U+3100..=U+312F).
    Bopomofo,
    /// `Hangul Compatibility Jamo` (U+3130..=U+318F).
    HangulCompatibilityJamo,
    /// `Kanbun` (U+3190..=U+319F).
    Kanbun,
    /// `Bopomofo Extended` (U+31A0..=U+31BF).
    BopomofoExtended,
    /// `CJK Strokes` (U+31C0..=U+31EF).
    CjkStrokes,
    /// `Katakana Phonetic Extensions` (U+31F0..=U+31FF).
    KatakanaPhoneticExtensions,
    /// `Enclosed CJK Letters and Months` (U+3200..=U+32FF).
    EnclosedCjkLettersAndMonths,
    /// `CJK Compatibility` (U+3300..=U+33FF).
    CjkCompatibility,
    /// `CJK Unified Ideographs Extension A` (U+3400..=U+4DBF).
    CjkUnifiedIdeographsExtensionA,
    /// `Yijing Hexagram Symbols` (U+4DC0..=U+4DFF).
    YijingHexagramSymbols,
    /// `CJK Unified Ideographs` (U+4E00..=U+9FFF).
    CjkUnifiedIdeographs,
    /// `Yi Syllables` (U+A000..=U+A48F).
    YiSyllables,
    /// `Yi Radicals` (U+A490..=U+A4CF).
    YiRadicals,
    /// `Lisu` (U+A4D0..=U+A4FF).
    Lisu,
    /// `Vai` (U+A500..=U+A63F).
    Vai,
    /// `Cyrillic Extended-B` (U+A640..=U+A69F).
    CyrillicExtendedB,
    /// `Bamum` (U+A6A0..=U+A6FF).
    Bamum,
    /// `Modifier Tone Letters` (U+A700..=U+A71F).
    ModifierToneLetters,
    /// `Latin Extended-D` (U+A720..=U+A7FF).
    LatinExtendedD,
    /// `Syloti Nagri` (U+A800..=U+A82F).
    SylotiNagri,
    /// `Common Indic Number Forms` (U+A830..=U+A83F).
    CommonIndicNumberForms,
    /// `Phags-pa` (U+A840..=U+A87F).
    PhagsPa,
    /// `Saurashtra` (U+A880..=U+A8DF).
    Saurashtra,
    /// `Devanagari Extended` (U+A8E0..=U+A8FF).
    DevanagariExtended,
    /// `Kayah Li` (U+A900..=U+A92F).
    KayahLi,
    /// `Rejang` (U+A930..=U+A95F).
    Rejang,
    /// `Hangul Jamo Extended-A` (U+A960..=U+A97F).
    HangulJamoExtendedA,
    /// `Javanese` (U+A980..=U+A9DF).
    Javanese,
    /// `Myanmar Extended-B` (U+A9E0..=U+A9FF).
    MyanmarExtendedB,
    /// `Cham` (U+AA00..=U+AA5F).
    Cham,
    /// `Myanmar Extended-A` (U+AA60..=U+AA7F).
    MyanmarExtendedA,
    /// `Tai Viet` (U+AA80..=U+AADF).
    TaiViet,
    /// `Meetei Mayek Extensions` (U+AAE0..=U+AAFF).
    MeeteiMayekExtensions,
    /// `Ethiopic Extended-A` (U+AB00..=U+AB2F).
    EthiopicExtendedA,
    /// `Latin Extended-E` (U+AB30..=U+AB6F).
    LatinExtendedE,
    /// `Cherokee Supplement` (U+AB70..=U+ABBF).
    CherokeeSupplement,
    /// `Meetei Mayek` (U+ABC0..=U+ABFF).
    MeeteiMayek,
    /// `Hangul Syllables` (U+AC00..=U+D7AF).
    HangulSyllables,
    /// `Hangul Jamo Extended-B` (U+D7B0..=U+D7FF).
    HangulJamoExtendedB,
    /// `High Surrogates` (U+D800..=U+DB7F).
    HighSurrogates,
    /// `High Private Use Surrogates` (U+DB80..=U+DBFF).
    HighPrivateUseSurrogates,
    /// `Low Surrogates` (U+DC00..=U+DFFF).
    LowSurrogates,
    /// `Private Use Area` (U+E000..=U+F8FF).
    PrivateUseArea,
    /// `CJK Compatibility Ideographs` (U+F900..=U+FAFF).
    CjkCompatibilityIdeographs,
    /// `Alphabetic Presentation Forms` (U+FB00..=U+FB4F).
    AlphabeticPresentationForms,
    /// `Arabic Presentation Forms-A` (U+FB50..=U+FDFF).
    ArabicPresentationFormsA,
    /// `Variation Selectors` (U+FE00..=U+FE0F).
    VariationSelectors,
    /// `Vertical Forms` (U+FE10..=U+FE1F).
    VerticalForms,
    /// `Combining Half Marks` (U+FE20..=U+FE2F).
    CombiningHalfMarks,
    /// `CJK Compatibility Forms` (U+FE30..=U+FE4F).
    CjkCompatibilityForms,
    /// `Small Form Variants` (U+FE50..=U+FE6F).
    SmallFormVariants,
    /// `Arabic Presentation Forms-B` (U+FE70..=U+FEFF).
    ArabicPresentationFormsB,
    /// `Halfwidth and Fullwidth Forms` (U+FF00..=U+FFEF).
    HalfwidthAndFullwidthForms,
    /// `Specials` (U+FFF0..=U+FFFF).
    Specials,
    /// `Linear B Syllabary` (U+10000..=U+1007F).
    LinearBSyllabary,
    /// `Linear B Ideograms` (U+10080..=U+100FF).
    LinearBIdeograms,
    /// `Aegean Numbers` (U+10100..=U+1013F).
    AegeanNumbers,
    /// `Ancient Greek Numbers` (U+10140..=U+1018F).
    AncientGreekNumbers,
    /// `Ancient Symbols` (U+10190..=U+101CF).
    AncientSymbols,
    /// `Phaistos Disc` (U+101D0..=U+101FF).
    PhaistosDisc,
    /// `Lycian` (U+10280..=U+1029F).
    Lycian,
    /// `Carian` (U+102A0..=U+102DF).
    Carian,
    /// `Coptic Epact Numbers` (U+102E0..=U+102FF).
    CopticEpactNumbers,
    /// `Old Italic` (U+10300..=U+1032F).
    OldItalic,
    /// `Gothic` (U+10330..=U+1034F).
    Gothic,
    /// `Old Permic` (U+10350..=U+1037F).
    OldPermic,
    /// `Ugaritic` (U+10380..=U+1039F).
    Ugaritic,
    /// `Old Persian` (U+103A0..=U+103DF).
    OldPersian,
    /// `Deseret` (U+10400..=U+1044F).
    Deseret,
    /// `Shavian` (U+10450..=U+1047F).
    Shavian,
    /// `Osmanya` (U+10480..=U+104AF).
    Osmanya,
    /// `Osage` (U+104B0..=U+104FF).
    Osage,
    /// `Elbasan` (U+10500..=U+1052F).
    Elbasan,
    /// `Caucasian Albanian` (U+10530..=U+1056F).
    CaucasianAlbanian,
    /// `Vithkuqi` (U+10570..=U+105BF).
    Vithkuqi,
    /// `Todhri` (U+105C0..=U+105FF).
    Todhri,
    /// `Linear A` (U+10600..=U+1077F).
    LinearA,
    /// `Latin Extended-F` (U+10780..=U+107BF).
    LatinExtendedF,
    /// `Cypriot Syllabary` (U+10800..=U+1083F).
    CypriotSyllabary,
    /// `Imperial Aramaic` (U+10840..=U+1085F).
    ImperialAramaic,
    /// `Palmyrene` (U+10860..=U+1087F).
    Palmyrene,
    /// `Nabataean` (U+10880..=U+108AF).
    Nabataean,
    /// `Hatran` (U+108E0..=U+108FF).
    Hatran,
    /// `Phoenician` (U+10900..=U+1091F).
    Phoenician,
    /// `Lydian` (U+10920..=U+1093F).
    Lydian,
    /// `Sidetic` (U+10940..=U+1095F).
    Sidetic,
    /// `Meroitic Hieroglyphs` (U+10980..=U+1099F).
    MeroiticHieroglyphs,
    /// `Meroitic Cursive` (U+109A0..=U+109FF).
    MeroiticCursive,
    /// `Kharoshthi` (U+10A00..=U+10A5F).
    Kharoshthi,
    /// `Old South Arabian` (U+10A60..=U+10A7F).
    OldSouthArabian,
    /// `Old North Arabian` (U+10A80..=U+10A9F).
    OldNorthArabian,
    /// `Manichaean` (U+10AC0..=U+10AFF).
    Manichaean,
    /// `Avestan` (U+10B00..=U+10B3F).
    Avestan,
    /// `Inscriptional Parthian` (U+10B40..=U+10B5F).
    InscriptionalParthian,
    /// `Inscriptional Pahlavi` (U+10B60..=U+10B7F).
    InscriptionalPahlavi,
    /// `Psalter Pahlavi` (U+10B80..=U+10BAF).
    PsalterPahlavi,
    /// `Old Turkic` (U+10C00..=U+10C4F).
    OldTurkic,
    /// `Old Hungarian` (U+10C80..=U+10CFF).
    OldHungarian,
    /// `Hanifi Rohingya` (U+10D00..=U+10D3F).
    HanifiRohingya,
    /// `Garay` (U+10D40..=U+10D8F).
    Garay,
    /// `Rumi Numeral Symbols` (U+10E60..=U+10E7F).
    RumiNumeralSymbols,
    /// `Yezidi` (U+10E80..=U+10EBF).
    Yezidi,
    /// `Arabic Extended-C` (U+10EC0..=U+10EFF).
    ArabicExtendedC,
    /// `Old Sogdian` (U+10F00..=U+10F2F).
    OldSogdian,
    /// `Sogdian` (U+10F30..=U+10F6F).
    Sogdian,
    /// `Old Uyghur` (U+10F70..=U+10FAF).
    OldUyghur,
    /// `Chorasmian` (U+10FB0..=U+10FDF).
    Chorasmian,
    /// `Elymaic` (U+10FE0..=U+10FFF).
    Elymaic,
    /// `Brahmi` (U+11000..=U+1107F).
    Brahmi,
    /// `Kaithi` (U+11080..=U+110CF).
    Kaithi,
    /// `Sora Sompeng` (U+110D0..=U+110FF).
    SoraSompeng,
    /// `Chakma` (U+11100..=U+1114F).
    Chakma,
    /// `Mahajani` (U+11150..=U+1117F).
    Mahajani,
    /// `Sharada` (U+11180..=U+111DF).
    Sharada,
    /// `Sinhala Archaic Numbers` (U+111E0..=U+111FF).
    SinhalaArchaicNumbers,
    /// `Khojki` (U+11200..=U+1124F).
    Khojki,
    /// `Multani` (U+11280..=U+112AF).
    Multani,
    /// `Khudawadi` (U+112B0..=U+112FF).
    Khudawadi,
    /// `Grantha` (U+11300..=U+1137F).
    Grantha,
    /// `Tulu-Tigalari` (U+11380..=U+113FF).
    TuluTigalari,
    /// `Newa` (U+11400..=U+1147F).
    Newa,
    /// `Tirhuta` (U+11480..=U+114DF).
    Tirhuta,
    /// `Siddham` (U+11580..=U+115FF).
    Siddham,
    /// `Modi` (U+11600..=U+1165F).
    Modi,
    /// `Mongolian Supplement` (U+11660..=U+1167F).
    MongolianSupplement,
    /// `Takri` (U+11680..=U+116CF).
    Takri,
    /// `Myanmar Extended-C` (U+116D0..=U+116FF).
    MyanmarExtendedC,
    /// `Ahom` (U+11700..=U+1174F).
    Ahom,
    /// `Dogra` (U+11800..=U+1184F).
    Dogra,
    /// `Warang Citi` (U+118A0..=U+118FF).
    WarangCiti,
    /// `Dives Akuru` (U+11900..=U+1195F).
    DivesAkuru,
    /// `Nandinagari` (U+119A0..=U+119FF).
    Nandinagari,
    /// `Zanabazar Square` (U+11A00..=U+11A4F).
    ZanabazarSquare,
    /// `Soyombo` (U+11A50..=U+11AAF).
    Soyombo,
    /// `Pau Cin Hau` (U+11AC0..=U+11AFF).
    PauCinHau,
    /// `Devanagari Extended-A` (U+11B00..=U+11B5F).
    DevanagariExtendedA,
    /// `Sharada Supplement` (U+11B60..=U+11B7F).
    SharadaSupplement,
    /// `Sunuwar` (U+11BC0..=U+11BFF).
    Sunuwar,
    /// `Bhaiksuki` (U+11C00..=U+11C6F).
    Bhaiksuki,
    /// `Marchen` (U+11C70..=U+11CBF).
    Marchen,
    /// `Masaram Gondi` (U+11D00..=U+11D5F).
    MasaramGondi,
    /// `Gunjala Gondi` (U+11D60..=U+11DAF).
    GunjalaGondi,
    /// `Tolong Siki` (U+11DB0..=U+11DEF).
    TolongSiki,
    /// `Makasar` (U+11EE0..=U+11EFF).
    Makasar,
    /// `Kawi` (U+11F00..=U+11F5F).
    Kawi,
    /// `Lisu Supplement` (U+11FB0..=U+11FBF).
    LisuSupplement,
    /// `Tamil Supplement` (U+11FC0..=U+11FFF).
    TamilSupplement,
    /// `Cuneiform` (U+12000..=U+123FF).
    Cuneiform,
    /// `Cuneiform Numbers and Punctuation` (U+12400..=U+1247F).
    CuneiformNumbersAndPunctuation,
    /// `Early Dynastic Cuneiform` (U+12480..=U+1254F).
    EarlyDynasticCuneiform,
    /// `Cypro-Minoan` (U+12F90..=U+12FFF).
    CyproMinoan,
    /// `Egyptian Hieroglyphs` (U+13000..=U+1342F).
    EgyptianHieroglyphs,
    /// `Egyptian Hieroglyph Format Controls` (U+13430..=U+1345F).
    EgyptianHieroglyphFormatControls,
    /// `Egyptian Hieroglyphs Extended-A` (U+13460..=U+143FF).
    EgyptianHieroglyphsExtendedA,
    /// `Anatolian Hieroglyphs` (U+14400..=U+1467F).
    AnatolianHieroglyphs,
    /// `Gurung Khema` (U+16100..=U+1613F).
    GurungKhema,
    /// `Bamum Supplement` (U+16800..=U+16A3F).
    BamumSupplement,
    /// `Mro` (U+16A40..=U+16A6F).
    Mro,
    /// `Tangsa` (U+16A70..=U+16ACF).
    Tangsa,
    /// `Bassa Vah` (U+16AD0..=U+16AFF).
    BassaVah,
    /// `Pahawh Hmong` (U+16B00..=U+16B8F).
    PahawhHmong,
    /// `Kirat Rai` (U+16D40..=U+16D7F).
    KiratRai,
    /// `Medefaidrin` (U+16E40..=U+16E9F).
    Medefaidrin,
    /// `Beria Erfe` (U+16EA0..=U+16EDF).
    BeriaErfe,
    /// `Miao` (U+16F00..=U+16F9F).
    Miao,
    /// `Ideographic Symbols and Punctuation` (U+16FE0..=U+16FFF).
    IdeographicSymbolsAndPunctuation,
    /// `Tangut` (U+17000..=U+187FF).
    Tangut,
    /// `Tangut Components` (U+18800..=U+18AFF).
    TangutComponents,
    /// `Khitan Small Script` (U+18B00..=U+18CFF).
    KhitanSmallScript,
    /// `Tangut Supplement` (U+18D00..=U+18D7F).
    TangutSupplement,
    /// `Tangut Components Supplement` (U+18D80..=U+18DFF).
    TangutComponentsSupplement,
    /// `Kana Extended-B` (U+1AFF0..=U+1AFFF).
    KanaExtendedB,
    /// `Kana Supplement` (U+1B000..=U+1B0FF).
    KanaSupplement,
    /// `Kana Extended-A` (U+1B100..=U+1B12F).
    KanaExtendedA,
    /// `Small Kana Extension` (U+1B130..=U+1B16F).
    SmallKanaExtension,
    /// `Nushu` (U+1B170..=U+1B2FF).
    Nushu,
    /// `Duployan` (U+1BC00..=U+1BC9F).
    Duployan,
    /// `Shorthand Format Controls` (U+1BCA0..=U+1BCAF).
    ShorthandFormatControls,
    /// `Symbols for Legacy Computing Supplement` (U+1CC00..=U+1CEBF).
    SymbolsForLegacyComputingSupplement,
    /// `Miscellaneous Symbols Supplement` (U+1CEC0..=U+1CEFF).
    MiscellaneousSymbolsSupplement,
    /// `Znamenny Musical Notation` (U+1CF00..=U+1CFCF).
    ZnamennyMusicalNotation,
    /// `Byzantine Musical Symbols` (U+1D000..=U+1D0FF).
    ByzantineMusicalSymbols,
    /// `Musical Symbols` (U+1D100..=U+1D1FF).
    MusicalSymbols,
    /// `Ancient Greek Musical Notation` (U+1D200..=U+1D24F).
    AncientGreekMusicalNotation,
    /// `Kaktovik Numerals` (U+1D2C0..=U+1D2DF).
    KaktovikNumerals,
    /// `Mayan Numerals` (U+1D2E0..=U+1D2FF).
    MayanNumerals,
    /// `Tai Xuan Jing Symbols` (U+1D300..=U+1D35F).
    TaiXuanJingSymbols,
    /// `Counting Rod Numerals` (U+1D360..=U+1D37F).
    CountingRodNumerals,
    /// `Mathematical Alphanumeric Symbols` (U+1D400..=U+1D7FF).
    MathematicalAlphanumericSymbols,
    /// `Sutton SignWriting` (U+1D800..=U+1DAAF).
    SuttonSignwriting,
    /// `Latin Extended-G` (U+1DF00..=U+1DFFF).
    LatinExtendedG,
    /// `Glagolitic Supplement` (U+1E000..=U+1E02F).
    GlagoliticSupplement,
    /// `Cyrillic Extended-D` (U+1E030..=U+1E08F).
    CyrillicExtendedD,
    /// `Nyiakeng Puachue Hmong` (U+1E100..=U+1E14F).
    NyiakengPuachueHmong,
    /// `Toto` (U+1E290..=U+1E2BF).
    Toto,
    /// `Wancho` (U+1E2C0..=U+1E2FF).
    Wancho,
    /// `Nag Mundari` (U+1E4D0..=U+1E4FF).
    NagMundari,
    /// `Ol Onal` (U+1E5D0..=U+1E5FF).
    OlOnal,
    /// `Tai Yo` (U+1E6C0..=U+1E6FF).
    TaiYo,
    /// `Ethiopic Extended-B` (U+1E7E0..=U+1E7FF).
    EthiopicExtendedB,
    /// `Mende Kikakui` (U+1E800..=U+1E8DF).
    MendeKikakui,
    /// `Adlam` (U+1E900..=U+1E95F).
    Adlam,
    /// `Indic Siyaq Numbers` (U+1EC70..=U+1ECBF).
    IndicSiyaqNumbers,
    /// `Ottoman Siyaq Numbers` (U+1ED00..=U+1ED4F).
    OttomanSiyaqNumbers,
    /// `Arabic Mathematical Alphabetic Symbols` (U+1EE00..=U+1EEFF).
    ArabicMathematicalAlphabeticSymbols,
    /// `Mahjong Tiles` (U+1F000..=U+1F02F).
    MahjongTiles,
    /// `Domino Tiles` (U+1F030..=U+1F09F).
    DominoTiles,
    /// `Playing Cards` (U+1F0A0..=U+1F0FF).
    PlayingCards,
    /// `Enclosed Alphanumeric Supplement` (U+1F100..=U+1F1FF).
    EnclosedAlphanumericSupplement,
    /// `Enclosed Ideographic Supplement` (U+1F200..=U+1F2FF).
    EnclosedIdeographicSupplement,
    /// `Miscellaneous Symbols and Pictographs` (U+1F300..=U+1F5FF).
    MiscellaneousSymbolsAndPictographs,
    /// `Emoticons` (U+1F600..=U+1F64F).
    Emoticons,
    /// `Ornamental Dingbats` (U+1F650..=U+1F67F).
    OrnamentalDingbats,
    /// `Transport and Map Symbols` (U+1F680..=U+1F6FF).
    TransportAndMapSymbols,
    /// `Alchemical Symbols` (U+1F700..=U+1F77F).
    AlchemicalSymbols,
    /// `Geometric Shapes Extended` (U+1F780..=U+1F7FF).
    GeometricShapesExtended,
    /// `Supplemental Arrows-C` (U+1F800..=U+1F8FF).
    SupplementalArrowsC,
    /// `Supplemental Symbols and Pictographs` (U+1F900..=U+1F9FF).
    SupplementalSymbolsAndPictographs,
    /// `Chess Symbols` (U+1FA00..=U+1FA6F).
    ChessSymbols,
    /// `Symbols and Pictographs Extended-A` (U+1FA70..=U+1FAFF).
    SymbolsAndPictographsExtendedA,
    /// `Symbols for Legacy Computing` (U+1FB00..=U+1FBFF).
    SymbolsForLegacyComputing,
    /// `CJK Unified Ideographs Extension B` (U+20000..=U+2A6DF).
    CjkUnifiedIdeographsExtensionB,
    /// `CJK Unified Ideographs Extension C` (U+2A700..=U+2B73F).
    CjkUnifiedIdeographsExtensionC,
    /// `CJK Unified Ideographs Extension D` (U+2B740..=U+2B81F).
    CjkUnifiedIdeographsExtensionD,
    /// `CJK Unified Ideographs Extension E` (U+2B820..=U+2CEAF).
    CjkUnifiedIdeographsExtensionE,
    /// `CJK Unified Ideographs Extension F` (U+2CEB0..=U+2EBEF).
    CjkUnifiedIdeographsExtensionF,
    /// `CJK Unified Ideographs Extension I` (U+2EBF0..=U+2EE5F).
    CjkUnifiedIdeographsExtensionI,
    /// `CJK Compatibility Ideographs Supplement` (U+2F800..=U+2FA1F).
    CjkCompatibilityIdeographsSupplement,
    /// `CJK Unified Ideographs Extension G` (U+30000..=U+3134F).
    CjkUnifiedIdeographsExtensionG,
    /// `CJK Unified Ideographs Extension H` (U+31350..=U+323AF).
    CjkUnifiedIdeographsExtensionH,
    /// `CJK Unified Ideographs Extension J` (U+323B0..=U+3347F).
    CjkUnifiedIdeographsExtensionJ,
    /// `Tags` (U+E0000..=U+E007F).
    Tags,
    /// `Variation Selectors Supplement` (U+E0100..=U+E01EF).
    VariationSelectorsSupplement,
    /// `Supplementary Private Use Area-A` (U+F0000..=U+FFFFF).
    SupplementaryPrivateUseAreaA,
    /// `Supplementary Private Use Area-B` (U+100000..=U+10FFFF).
    SupplementaryPrivateUseAreaB,
    /// Codepoint not inside any assigned block (gaps between blocks).
    Unassigned,
}

// ---------------------------------------------------------------------
// Convenience group predicates (UCD §4.5 top-level groups).
// ---------------------------------------------------------------------

/// Returns `true` iff `c` is in General_Category group `L`
/// (`Lu`, `Ll`, `Lt`, `Lm`, `Lo`).
pub fn is_letter(c: char) -> bool {
    matches!(
        general_category(c),
        GeneralCategory::Lu
            | GeneralCategory::Ll
            | GeneralCategory::Lt
            | GeneralCategory::Lm
            | GeneralCategory::Lo
    )
}

/// Returns `true` iff `c` is in General_Category group `M`
/// (`Mn`, `Mc`, `Me`).
pub fn is_mark(c: char) -> bool {
    matches!(
        general_category(c),
        GeneralCategory::Mn | GeneralCategory::Mc | GeneralCategory::Me
    )
}

/// Returns `true` iff `c` is in General_Category group `N`
/// (`Nd`, `Nl`, `No`).
pub fn is_number(c: char) -> bool {
    matches!(
        general_category(c),
        GeneralCategory::Nd | GeneralCategory::Nl | GeneralCategory::No
    )
}

/// Returns `true` iff `c` is in General_Category group `P`
/// (`Pc`, `Pd`, `Ps`, `Pe`, `Pi`, `Pf`, `Po`).
pub fn is_punctuation(c: char) -> bool {
    matches!(
        general_category(c),
        GeneralCategory::Pc
            | GeneralCategory::Pd
            | GeneralCategory::Ps
            | GeneralCategory::Pe
            | GeneralCategory::Pi
            | GeneralCategory::Pf
            | GeneralCategory::Po
    )
}

/// Returns `true` iff `c` is in General_Category group `S`
/// (`Sm`, `Sc`, `Sk`, `So`).
pub fn is_symbol(c: char) -> bool {
    matches!(
        general_category(c),
        GeneralCategory::Sm
            | GeneralCategory::Sc
            | GeneralCategory::Sk
            | GeneralCategory::So
    )
}

/// Returns `true` iff `c` is in General_Category group `Z`
/// (`Zs`, `Zl`, `Zp`).
pub fn is_separator(c: char) -> bool {
    matches!(
        general_category(c),
        GeneralCategory::Zs | GeneralCategory::Zl | GeneralCategory::Zp
    )
}

/// Returns `true` iff `c` is in General_Category group `C`
/// (`Cc`, `Cf`, `Cs`, `Co`, `Cn`).
pub fn is_other(c: char) -> bool {
    matches!(
        general_category(c),
        GeneralCategory::Cc
            | GeneralCategory::Cf
            | GeneralCategory::Cs
            | GeneralCategory::Co
            | GeneralCategory::Cn
    )
}
