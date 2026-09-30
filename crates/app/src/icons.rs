//! Icon names stored in the database, resolved to Phosphor glyphs.

pub use egui_phosphor::regular as ph;

/// Icons offered in the category / goal icon pickers.
pub const PICKER: &[(&str, &str)] = &[
    ("shopping-cart", ph::SHOPPING_CART),
    ("fork-knife", ph::FORK_KNIFE),
    ("coffee", ph::COFFEE),
    ("pizza", ph::PIZZA),
    ("hamburger", ph::HAMBURGER),
    ("wine", ph::WINE),
    ("car", ph::CAR),
    ("gas-pump", ph::GAS_PUMP),
    ("bus", ph::BUS),
    ("train", ph::TRAIN),
    ("bicycle", ph::BICYCLE),
    ("airplane", ph::AIRPLANE),
    ("airplane-tilt", ph::AIRPLANE_TILT),
    ("house", ph::HOUSE),
    ("lightning", ph::LIGHTNING),
    ("wifi", ph::WIFI_HIGH),
    ("device-mobile", ph::DEVICE_MOBILE),
    ("television", ph::TELEVISION),
    ("repeat", ph::REPEAT),
    ("bag", ph::BAG),
    ("t-shirt", ph::T_SHIRT),
    ("storefront", ph::STOREFRONT),
    ("heartbeat", ph::HEARTBEAT),
    ("pill", ph::PILL),
    ("barbell", ph::BARBELL),
    ("film-strip", ph::FILM_STRIP),
    ("game-controller", ph::GAME_CONTROLLER),
    ("music-notes", ph::MUSIC_NOTES),
    ("book", ph::BOOK),
    ("graduation-cap", ph::GRADUATION_CAP),
    ("gift", ph::GIFT),
    ("sparkle", ph::SPARKLE),
    ("paw-print", ph::PAW_PRINT),
    ("plant", ph::PLANT),
    ("receipt", ph::RECEIPT),
    ("shield-check", ph::SHIELD_CHECK),
    ("briefcase", ph::BRIEFCASE),
    ("laptop", ph::LAPTOP),
    ("bank", ph::BANK),
    ("coins", ph::COINS),
    ("hand-coins", ph::HAND_COINS),
    ("piggy-bank", ph::PIGGY_BANK),
    ("lifebuoy", ph::LIFEBUOY),
    ("target", ph::TARGET),
    ("trophy", ph::TROPHY),
    ("tag", ph::TAG),
];

pub fn glyph(name: &str) -> &'static str {
    PICKER
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, g)| *g)
        .unwrap_or(ph::TAG)
}

pub fn account_kind(kind: magpie_core::AccountKind) -> &'static str {
    use magpie_core::AccountKind::*;
    match kind {
        Checking => ph::BANK,
        Savings => ph::PIGGY_BANK,
        Credit => ph::CREDIT_CARD,
        Cash => ph::WALLET,
        Investment => ph::CHART_LINE_UP,
    }
}
