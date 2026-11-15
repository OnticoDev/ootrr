//! Turns off the game's tutorials (action guides).
//!
//! Every tutorial an event flow shows goes through Event/EventFlow/ActionGuide: about 25
//! callers (the item get and chest flows, signs, Kokiri children, Navi's hints, shops, the
//! fishing pond, the Great Fairy, the first Hyrule Field arrival). Its five entries only open
//! the guide screens, request tutorials, clear their own "guide still to show" flags, switch
//! the pause menu tab and, for the ocarina, start the Dm_SY_Ocarina playing lesson. Nothing
//! in it matters to the game, so each entry now runs a one frame wait and ends.
//!
//! This also keeps the ocarina lesson from starting when Link has no ocarina, which a shuffled
//! Saria's gift allows.
//!
//! The game code also opens some guides itself, without an event flow (for example "press RT
//! to use Zelda's Letter" when an assignable item is first picked up). Each of the 53 guides
//! has a save flag, ActionGuide_IsEnd.<guide>, that marks it as done, so all of them are set
//! at new game ([`start_flags`]).

use crate::bfevfl::{Bf, Cont, Event, Flow, Kind, Val};
use crate::error::{Error, Res};
use crate::romfs::Romfs;
use crate::staging::{self, Out};

pub const FILE: &str = "Event/EventFlow/ActionGuide.bfevfl.zs";
pub const FLOW: &str = "ActionGuide";
pub const PREFIX: &str = "OOTRRNoGuide";
pub const ENTRIES: [&str; 5] = ["OpenActionGuideSceen", "OpenActionGuideSceenForce", "ActionGuide", "ActionGuideForShop", "OpenActionGuideSceenForOcarina"];
const NONE: u16 = 0xFFFF;
const TRUE: u32 = 0x8000_0001;

/// Every action guide, as named by its ActionGuide_IsEnd flag (and in LayoutMsg/ActionGuide_00).
pub const GUIDES: [&str; 53] = [
    "ActivationAmiibo", "AgonyStone", "AlarmCucco", "Amiibo", "Backflip", "Bomb", "Bombchu", "Boomerang", "BootShortcut", "Bottle", "Bow", "DekuNut", "DekuStick", "DinFire",
    "EquipmentMenu", "EquipmentMenuForBoot", "EquipmentMenuForHeadArmor", "FaroreWind", "FireArrow", "FishingRod", "GoronGauntlet", "Hammer", "HeadShortcut", "Hookshot",
    "HorizontalSlash", "IceArrow", "ItemMenu", "ItemShortcut", "JumpSlash", "LightArrow", "LockOn", "LockOnShield", "LockOnSideMove", "Longshot", "MagicBean", "Navi",
    "NayruLove", "Ocarina", "OcarinaSongEstimate", "RollAttack", "RotarySlash", "Save", "ScrubBusinessSeed", "SideJump", "SinkingLure", "SlingShot", "SystemMinus",
    "Thrust", "TimeTrail", "VerticalSlash", "Weapon_Shield_Hylian", "Weapon_Shield_Mirror", "ZeldaLetter",
];

/// New game flags that mark every guide as already done.
pub fn start_flags() -> Vec<(String, bool)> {
    GUIDES.iter().map(|g| (format!("ActionGuide_IsEnd.{g}"), true)).collect()
}

/// Points every entry at one new wait event that ends the flow. Returns the entries changed.
pub fn patch(b: &mut Bf) -> Res<usize> {
    let f: &mut Flow = b.flows.iter_mut().find(|f| f.name == FLOW).ok_or(Error::Format("no tutorials: no ActionGuide flow"))?;
    if f.events.iter().any(|e| e.name.starts_with(PREFIX)) {
        return Err(Error::Format("no tutorials: ActionGuide was already patched"));
    }
    let names: Vec<&str> = f.entries.iter().map(|e| e.name.as_str()).collect();
    if names != ENTRIES {
        return Err(Error::Msg(format!("no tutorials: ActionGuide's entries are {names:?}, the game files differ")));
    }
    let (a, x): (u16, u16) = crate::start_state::slot(f, "EventSystemActor", "EventWait")?;
    let end: usize = f.events.len();
    if end >= NONE as usize {
        return Err(Error::Format("no tutorials: too many events in the flow"));
    }
    f.events.push(Event {
        name: format!("{PREFIX}{end}"),
        kind: Kind::Action {
            next: NONE,
            actor: a,
            act: x,
            params: Some(Cont { items: vec![("IsWaitEnd".to_string(), Val::Bool(TRUE)), ("Frames".to_string(), Val::Int(1))] }),
        },
    });
    for e in &mut f.entries {
        e.main = end as u16;
        e.subs.clear();
    }
    Ok(f.entries.len())
}

pub fn stage(rf: &mut Romfs) -> Res<Out> {
    staging::rewrite(rf, FILE, |raw| {
        let mut b: Bf = Bf::parse(raw)?;
        patch(&mut b)?;
        let out: Vec<u8> = b.write()?;
        if Bf::parse(&out)? != b {
            return Err(Error::Format("no tutorials: the patched flow does not parse back"));
        }
        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_of_the_real_action_guide_ends_at_once() {
        let p: std::path::PathBuf = std::env::var_os("OOTRR_ROMFS").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from("C:/o/nup dump/updates/131072/romfs"));
        if !p.is_dir() {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", p.display());
            return;
        }
        let mut rf: Romfs = Romfs::open(&p).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(FILE).unwrap()).unwrap();
        assert_eq!(patch(&mut b).unwrap(), ENTRIES.len());
        assert!(patch(&mut b).is_err());
        let f: &Flow = &b.flows[0];
        let end: u16 = f.entries[0].main;
        assert!(f.entries.iter().all(|e| e.main == end && e.subs.is_empty()));
        let Kind::Action { next, .. } = &f.events[end as usize].kind else { panic!() };
        assert_eq!(*next, NONE);
        assert_eq!(Bf::parse(&b.write().unwrap()).unwrap(), b);
    }
}
