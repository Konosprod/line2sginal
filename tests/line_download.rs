//! Tests réseau sur de vrais packs LINE : `cargo test -- --ignored`.

use line2signal::convert::{apng_info, convert, FitMode};
use line2signal::line::fetch_pack;

#[test]
#[ignore = "accès réseau"]
fn static_pack() {
    // « Sally: Special Edition »
    let pack = fetch_pack(789, &|_, _| {}).unwrap();
    assert_eq!(pack.stickers.len(), 40);
    assert!(pack.stickers.iter().all(|s| !s.animated));
    let c = convert(&pack.stickers[0].data, FitMode::default()).unwrap();
    assert!(c.fits() && !c.animated);
}

#[test]
#[ignore = "accès réseau"]
fn animated_pack() {
    // « Brown & Cony & Sally: Animated Special »
    let pack = fetch_pack(11537, &|_, _| {}).unwrap();
    assert_eq!(pack.stickers.len(), 40);
    assert!(pack.stickers.iter().all(|s| s.animated));
    for s in &pack.stickers {
        let c = convert(&s.data, FitMode::default()).unwrap();
        assert!(c.fits(), "{} : {} octets", s.id, c.bytes.len());
        assert_eq!(
            apng_info(&c.bytes).map(|i| i.1),
            Some(0),
            "{} doit boucler",
            s.id
        );
    }
}
