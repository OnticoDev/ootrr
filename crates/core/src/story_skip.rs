//! Story cutscenes skipped by writing what they would have written.
//!
//! - Deku Tree (with an open forest): the meeting (Dm_Kokiri_0070/0075) and the first arrival
//!   inside the tree (Dm_Deku_0000 plus Navi's dungeon lesson) are replaced by their writes at
//!   new game, so the mouth is open and the quest is where the arrival leaves it.
//! - Saria's gift (setting): the bridge event goes straight to Dm_Kokiri_0155, the short gift
//!   scene that hands over the check's item, the way it does when the long Dm_Kokiri_0150 scene
//!   is skipped with the button. 0150 is marked as played there, which retires the trigger.
//!   0155 itself is cut down to its grant and get window, a plain item get.
//! - Temple of Time as a child (setting): the first entry and the altar scenes are marked as
//!   played at new game, with the flags the altar scene leaves behind.
//! - First Master Sword draw (setting): the draw gives the sword and the item of Rauru's check,
//!   writes what the Rauru and Sheik scenes write, and hands over to the game's own later-draw
//!   event (Market_Child_Field_ChildToAdult), which swaps the equipment, makes Link an adult and
//!   loads the adult temple.
//!
//! Nothing here adds a call into another flow file. Such a call is only loaded when the event's
//! row in RSDB/GameAnalyzedEventInfo lists the file, and even with the row edited a get-item
//! call spliced into Saria's bridge event hung the game (2026-11-15, twice).

use std::fs;

use crate::bfevfl::{Bf, Cont, Event, Flow, Kind, Val};
use crate::error::{Error, Res};
use crate::qol::{edit_zs, Have};
use crate::romfs::Romfs;
use crate::settings::Settings;
use crate::staging::Out;
use crate::start_state::Ops;

pub const SETTING: &str = "skip_story_scenes";

pub const NOTE_DEKU: &str = "The Deku Tree is met at new game: its mouth is open, the Deku Tree quest is complete and the Forest Stone quest is where Link's first arrival inside the tree leaves it, so the meeting, the arrival scene and Navi's dungeon lesson are skipped.";
pub const NOTE: &str = "Story cutscenes are skipped. Saria's bridge scenes are cut: crossing the bridge gives the check's item with a plain item get window. The Temple of Time's first entry and altar scenes are marked as played. The first Master Sword draw gives the sword and the item of Rauru's check and then plays the short scene of a later draw, without the sword pull, Ganondorf, Rauru or Sheik scenes.";

const TRUE: u32 = 0x8000_0001;
const FALSE: u32 = 0;
const NONE: u16 = 0xFFFF;

const DATA_ACTOR: &str = "GameDataActor";
const SET_BOOL: &str = "EventTriggerSetGameDataBool";
const CHALLENGE_ACTOR: &str = "ChallengeSystemActor";
const PROGRESS: &str = "EventExecuteProgressChallengeStep";

// ---------------------------------------------------------------- Deku Tree (open forest)

/// Steps after the anchor (Meet_DekuTree, which the open forest already moves to Step03):
/// the meeting completes the Deku Tree quest and opens the Forest Stone quest, the first
/// arrival inside the tree moves it to MeetBoss.
pub const DEKU_STEPS: [(&str, &str); 3] = [("Meet_DekuTree", "Complete"), ("Get_StoneOfForest", "GoInside"), ("Get_StoneOfForest", "MeetBoss")];
pub const DEKU_ENUMS: [(&str, &str); 2] = [("Step_Meet_DekuTree", "Complete"), ("Step_Get_StoneOfForest", "MeetBoss")];
/// The mouth flag is what Dm_Kokiri_0075 writes (the tree model reads it). The dungeon entrance
/// plays its arrival only while Dm_Deku_0000 is unplayed; that arrival also runs Navi's dungeon
/// lesson, which gates the D-pad Navi talk, so the lesson's markers are written too (the same
/// four the Kakariko state writes).
pub const DEKU_FLAGS: [&str; 8] = [
    "KokiriForest_Meet_DekuTree_OpenDekuTreeMouth",
    "HavePlayedEvent.Dm_Kokiri_0070",
    "HavePlayedEvent.Dm_Kokiri_0075",
    "HavePlayedEvent.Dm_Deku_0000",
    "HavePlayedEvent.NaviHint_Trial",
    "HaveTalked.Npc_Navi",
    "ActionGuide_IsEnd.Navi",
    "ActionGuide_IsNew.Navi",
];

pub fn deku() -> Ops {
    Ops {
        steps: DEKU_STEPS.iter().map(|(c, s)| ((*c).to_string(), (*s).to_string())).collect(),
        enums: DEKU_ENUMS.iter().map(|(c, s)| ((*c).to_string(), (*s).to_string())).collect(),
        flags: DEKU_FLAGS.iter().map(|n| ((*n).to_string(), true)).collect(),
        ..Ops::default()
    }
}

// ---------------------------------------------------------------- new game state (setting)

/// The child Temple of Time: each trigger area exists only while its scene is unplayed. The
/// altar scene's flow clears the first-time light shaft and its area's logic sets the found flag.
pub const TEMPLE_FLAGS: [&str; 4] = [
    "HavePlayedEvent.Dm_TimeTemple_0000",
    "HavePlayedEvent.Dm_TimeTemple_0020",
    "Market_Child_TimeTemple_DeleteLightShaft_FirstTime_00",
    "Market_Child_FoundMasterSword",
];

/// The new game flags of the setting.
pub fn start() -> Ops {
    Ops { flags: TEMPLE_FLAGS.iter().map(|n| ((*n).to_string(), true)).collect(), ..Ops::default() }
}

pub fn read(s: &Settings, bad: &mut Vec<String>) -> bool {
    s.flag(SETTING).unwrap_or_else(|e| {
        bad.push(e.to_string());
        false
    })
}

// ---------------------------------------------------------------- flow helpers

fn flag(v: bool) -> Val {
    Val::Bool(if v { TRUE } else { FALSE })
}

fn item(k: &str, v: Val) -> (String, Val) {
    (k.to_string(), v)
}

fn idx(n: usize) -> Res<u16> {
    if n >= NONE as usize {
        return Err(Error::Format("story skip: too many events in the flow"));
    }
    Ok(n as u16)
}

fn find(f: &Flow, name: &str) -> Res<usize> {
    f.events
        .iter()
        .position(|e| e.name == name)
        .ok_or_else(|| Error::Msg(format!("story skip: {} has no event {name}, the game files differ", f.name)))
}

fn text<'a>(e: &'a Event, k: &str) -> Option<&'a str> {
    match e.kind.params()?.get(k)? {
        Val::Str(s) => Some(s.as_str()),
        _ => None,
    }
}

/// The action an event runs, as `actor.action`.
fn action(f: &Flow, i: usize) -> Option<String> {
    let Kind::Action { actor, act, .. } = &f.events[i].kind else {
        return None;
    };
    let a = f.actors.get(*actor as usize)?;
    Some(format!("{}.{}", a.name, a.actions.get(*act as usize)?))
}

fn next_of(k: &Kind) -> Option<u16> {
    match k {
        Kind::Action { next, .. } | Kind::Sub { next, .. } | Kind::Join { next } => Some(*next),
        _ => None,
    }
}

fn set_next(k: &mut Kind, to: u16) -> Res<()> {
    match k {
        Kind::Action { next, .. } | Kind::Sub { next, .. } | Kind::Join { next } => {
            *next = to;
            Ok(())
        }
        _ => Err(Error::Format("story skip: the event has no single next")),
    }
}

/// Appends a chain of events, each pointing at the following one and the last at `then`.
/// Returns the index of the first.
fn chain(f: &mut Flow, prefix: &str, kinds: Vec<Kind>, then: u16) -> Res<u16> {
    let base: usize = f.events.len();
    idx(base + kinds.len())?;
    let n: usize = kinds.len();
    for (i, mut k) in kinds.into_iter().enumerate() {
        set_next(&mut k, if i + 1 < n { idx(base + i + 1)? } else { then })?;
        f.events.push(Event { name: format!("{prefix}{}", base + i), kind: k });
    }
    idx(base)
}

fn set_bool(f: &mut Flow, name: &str, v: bool) -> Res<Kind> {
    let (a, x) = crate::start_state::slot(f, DATA_ACTOR, SET_BOOL)?;
    Ok(Kind::Action { next: NONE, actor: a, act: x, params: Some(Cont { items: vec![item("Index", Val::Int(-1)), item("GameDataName", Val::Str(name.to_string())), item("Value", flag(v))] }) })
}

fn step(f: &mut Flow, quest: &str, step: &str) -> Res<Kind> {
    let (a, x) = crate::start_state::slot(f, CHALLENGE_ACTOR, PROGRESS)?;
    Ok(Kind::Action {
        next: NONE,
        actor: a,
        act: x,
        params: Some(Cont {
            items: vec![
                item("ChallengeName", Val::Str(quest.to_string())),
                item("StepName", Val::Str(step.to_string())),
                item("IsIgnoreCompletedChallenge", flag(false)),
                item("IsIgnoreChallengeProgressUI", flag(true)),
                item("IsWaitEnd", flag(true)),
            ],
        }),
    })
}

fn guard(f: &Flow, prefix: &str) -> Res<()> {
    if f.events.iter().any(|e| e.name.starts_with(prefix)) {
        return Err(Error::Msg(format!("story skip: {} was already patched", f.name)));
    }
    Ok(())
}

fn flow_mut<'a>(b: &'a mut Bf, name: &str) -> Res<&'a mut Flow> {
    b.flows.iter_mut().find(|f| f.name == name).ok_or_else(|| Error::Msg(format!("story skip: no flow {name}")))
}

fn rebuild(raw: &[u8], edit: impl FnOnce(&mut Bf) -> Res<()>) -> Res<Vec<u8>> {
    let mut b: Bf = Bf::parse(raw)?;
    edit(&mut b)?;
    let out: Vec<u8> = b.write()?;
    if Bf::parse(&out)? != b {
        return Err(Error::Format("story skip: the patched flow does not parse back to the edited model"));
    }
    Ok(out)
}

/// The string parameter `param` of event `index` (checked by name) in a flow file.
fn param_of(raw: &[u8], index: usize, event: &str, param: &str) -> Res<String> {
    let b: Bf = Bf::parse(raw)?;
    let f: &Flow = b.flows.first().ok_or(Error::Format("story skip: the file holds no flow"))?;
    let e: &Event = f.events.get(index).ok_or_else(|| Error::Msg(format!("story skip: {} has no event {index}", f.name)))?;
    if e.name != event {
        return Err(Error::Msg(format!("story skip: event {index} of {} is {} and not {event}, the game files differ", f.name, e.name)));
    }
    text(e, param).map(str::to_string).ok_or_else(|| Error::Msg(format!("story skip: {event} of {} has no {param}", f.name)))
}

fn copy(f: &Flow, name: &str, want: &str) -> Res<Kind> {
    let i: usize = find(f, name)?;
    let k: Kind = f.events[i].kind.clone();
    let ok: bool = match &k {
        Kind::Sub { entry, .. } => entry == want,
        Kind::Action { .. } => action(f, i).as_deref() == Some(want),
        _ => false,
    };
    if !ok {
        return Err(Error::Msg(format!("story skip: {name} in {} is not {want}, the game files differ", f.name)));
    }
    Ok(k)
}

fn set_param(k: &mut Kind, key: &str, v: Val) -> Res<()> {
    let c: &mut Cont = k.params_mut().ok_or(Error::Format("story skip: the event has no parameters"))?;
    let slot: &mut Val = c.items.iter_mut().find(|(n, _)| n == key).map(|(_, v)| v).ok_or_else(|| Error::Msg(format!("story skip: the event has no {key}")))?;
    *slot = v;
    Ok(())
}

// ---------------------------------------------------------------- Saria's gift

pub const BRIDGE_FILE: &str = "Event/EventFlow/Goto_HyruleCastle.bfevfl.zs";
pub const BRIDGE_FLOW: &str = "Goto_HyruleCastle";
pub const BRIDGE_PREFIX: &str = "OOTRRSaria";
/// KokiriForest_ArriveLostWood: sound ducking -> Dm_Kokiri_0150 -> (skipped? fade out) ->
/// sound release -> Dm_Kokiri_0155 (the gift) -> ...
const DUCKING: &str = "Event58";
const LONG_SCENE: &str = "Event43";
const RELEASE: &str = "Event115";
const GIFT_SCENE: &str = "Event44";
/// The bridge trigger (KokiriForest_Child_Level_All_02_ddcd) is deleted once Dm_Kokiri_0150 has
/// played. Skipping the transition means the game never marks it, so the flow does.
pub const LONG_FLAG: &str = "HavePlayedEvent.Dm_Kokiri_0150";

/// The ducking now leads to a write of the 0150 flag and then to the sound release, which goes
/// on to the gift scene. Both scenes stay in the event's preload row.
pub fn patch_bridge(b: &mut Bf) -> Res<()> {
    let f: &mut Flow = flow_mut(b, BRIDGE_FLOW)?;
    guard(f, BRIDGE_PREFIX)?;
    let duck: usize = find(f, DUCKING)?;
    let long: usize = find(f, LONG_SCENE)?;
    let release: usize = find(f, RELEASE)?;
    let gift: usize = find(f, GIFT_SCENE)?;
    if text(&f.events[long], "EventName") != Some("Dm_Kokiri_0150") || text(&f.events[gift], "EventName") != Some("Dm_Kokiri_0155") {
        return Err(Error::Format("story skip: the bridge scenes are not where they were, the game files differ"));
    }
    if next_of(&f.events[duck].kind) != Some(idx(long)?) || next_of(&f.events[release].kind) != Some(idx(gift)?) {
        return Err(Error::Format("story skip: the bridge flow no longer runs ducking -> 0150 and release -> 0155"));
    }
    let k: Kind = set_bool(f, LONG_FLAG, true)?;
    let first: u16 = chain(f, BRIDGE_PREFIX, vec![k], idx(release)?)?;
    set_next(&mut f.events[duck].kind, first)?;
    Ok(())
}

/// The short gift scene. Its C01 cut shows the get window (Event77) and then, inside a fork,
/// adds the item (Event68); the location patcher points both at the check's item.
pub const GIFT_FILE: &str = "Event/EventFlow/Dm_Kokiri_0155.bfevfl.zs";
pub const GIFT_FLOW: &str = "Dm_Kokiri_0155";
pub const GIFT_ENTRY: &str = "EntryPoint0";
const GIFT_GRANT: &str = "Event68";
const GIFT_SHOW: &str = "Event77";
const POUCH_ADD: &str = "PouchSystemActor.EventTriggerAddOrRemovePouchContent";

/// Turns the gift scene into a plain item get: the entry point runs the grant, then the get
/// window, then ends, with no cuts, camera or animation.
///
/// The gift stays in its own scene file on purpose. A get-item call spliced into the bridge
/// event (Goto_HyruleCastle) hung the game twice, even with its preload row edited. This scene
/// already calls GetPouchContent:DisplayGetByActorName, so nothing new has to be loaded.
pub fn patch_gift(b: &mut Bf) -> Res<()> {
    let f: &mut Flow = flow_mut(b, GIFT_FLOW)?;
    let grant: usize = find(f, GIFT_GRANT)?;
    let show: usize = find(f, GIFT_SHOW)?;
    if action(f, grant).as_deref() != Some(POUCH_ADD) {
        return Err(Error::Format("story skip: the gift scene's grant is not a pouch add, the game files differ"));
    }
    if !matches!(&f.events[show].kind, Kind::Sub { flow, entry, .. } if flow == "GetPouchContent" && entry == "DisplayGetByActorName") {
        return Err(Error::Format("story skip: the gift scene's get window is not GetPouchContent:DisplayGetByActorName, the game files differ"));
    }
    let (g, s): (u16, u16) = (idx(grant)?, idx(show)?);
    let e: &mut crate::bfevfl::Entry = f.entries.iter_mut().find(|e| e.name == GIFT_ENTRY).ok_or(Error::Format("story skip: the gift scene has no EntryPoint0"))?;
    if e.main == g {
        return Err(Error::Format("story skip: the gift scene was already patched"));
    }
    e.main = g;
    e.subs = vec![s];
    set_next(&mut f.events[grant].kind, s)?;
    set_next(&mut f.events[show].kind, NONE)?;
    Ok(())
}

// ---------------------------------------------------------------- first Master Sword draw

pub const DOOR_FILE: &str = "Event/EventFlow/DoorOfTime.bfevfl.zs";
pub const DOOR_FLOW: &str = "DoorOfTime";
pub const DOOR_PREFIX: &str = "OOTRRDraw";
/// DoorOfTime_Draw: true when Ganondorf's scene has played (a later draw), false on the first.
const DRAW_SWITCH: &str = "Event23";
/// The later-draw branch: a transition to Market_Child_Field_ChildToAdult.
const LATER_DRAW: &str = "Event22";
const GIVE_SWORD: &str = "Event115";
/// The Recovery sub that follows the adult temple load in Rauru's path; door_fix guards the
/// quest completion after it.
const RECOVERY: &str = "Event102";
pub const LATER_EVENT: &str = "Market_Child_Field_ChildToAdult";
/// Rauru's grant, which the location patcher points at the check's item.
pub const RAURU_FILE: &str = "Event/EventFlow/Dm_SageRoom_0005.bfevfl.zs";
pub const RAURU_INDEX: usize = 182;
pub const RAURU_EVENT: &str = "Event48";
pub const RAURU_PARAM: &str = "ActorName";
/// Every later draw tells itself apart from the first by Dm_TimeTemple_0040. The rest are the
/// skipped scenes (Sheik's altar talk 0065 would start while the sages quest is at Step01) and
/// what Market_Adult_AllSage_Start writes after Sheik's scenes.
pub const DRAW_FLAGS: [&str; 9] = [
    "HavePlayedEvent.Dm_TimeTemple_0030",
    "HavePlayedEvent.Dm_TimeTemple_0040",
    "HavePlayedEvent.Dm_SageRoom_0000",
    "HavePlayedEvent.Dm_SageRoom_0005",
    "HavePlayedEvent.Dm_TimeTemple_0050",
    "HavePlayedEvent.Dm_TimeTemple_0055",
    "HavePlayedEvent.Dm_TimeTemple_0065",
    "Market_Child_Firsttime_GotoAdult",
    "HavePlayedEvent.NaviHint_Shield_Hylian",
];
/// The quest steps Market_Adult_AllSage_Start writes, in its order.
pub const DRAW_STEPS: [(&str, &str); 3] = [("SageOfLight", "Complete"), ("AllSage", "Step01"), ("SageOfForest", "Step01")];

/// The first draw went pull scene -> Chamber of Sages (Ganondorf, the sword, the era change)
/// -> Chamber of Sages (Rauru, his check) -> adult temple (Sheik, quest steps). It now writes
/// the flags, gives the sword and `medal` (plus its IsGet flag `got`, which a pouch add without
/// the get event may leave unset), writes the quest steps and transitions to the later-draw
/// event, which the draw's own event table row already lists.
///
/// The transition is the last thing the draw does, as in the later draw. A first version ran
/// Rauru's Recovery sub (a wait and a heal) after it and the game crashed while the child temple
/// was reloaded with Link already an adult (2026-11-15). The sword is not equipped directly
/// either: the era swap equips it for the adult, as on every later draw. The guarded quest
/// completion that followed Recovery (door_fix) runs before the transition instead.
pub fn patch_draw(b: &mut Bf, medal: &str, got: Option<&str>) -> Res<()> {
    let f: &mut Flow = flow_mut(b, DOOR_FLOW)?;
    guard(f, DOOR_PREFIX)?;
    let sw: usize = find(f, DRAW_SWITCH)?;
    let Kind::Switch { cases, params, .. } = &f.events[sw].kind else {
        return Err(Error::Format("story skip: the draw check is not a switch"));
    };
    if params.as_ref().and_then(|c| c.get("EventName")) != Some(&Val::Str("Dm_TimeTemple_0040".to_string())) || cases.len() != 2 {
        return Err(Error::Format("story skip: the draw check does not test Dm_TimeTemple_0040, the game files differ"));
    }
    let later: usize = find(f, LATER_DRAW)?;
    if text(&f.events[later], "EventName") != Some(LATER_EVENT) || action(f, later).as_deref() != Some("EventSystemActor.EventTransition") {
        return Err(Error::Format("story skip: the later draw no longer transitions to Market_Child_Field_ChildToAdult"));
    }
    let mut kinds: Vec<Kind> = Vec::new();
    for n in DRAW_FLAGS {
        kinds.push(set_bool(f, n, true)?);
    }
    let mut sword: Kind = copy(f, GIVE_SWORD, "PouchSystemActor.EventTriggerAddOrRemovePouchContent")?;
    if text(&f.events[find(f, GIVE_SWORD)?], "Actor") != Some("Weapon_Sword_Master") {
        return Err(Error::Format("story skip: the Ganondorf path no longer gives the Master Sword"));
    }
    set_param(&mut sword, "IsEquipDIrect", flag(false))?;
    let mut gift: Kind = sword.clone();
    set_param(&mut gift, "Actor", Val::Str(medal.to_string()))?;
    kinds.push(sword);
    kinds.push(gift);
    if let Some(n) = got {
        kinds.push(set_bool(f, n, true)?);
    }
    for (q, s) in DRAW_STEPS {
        kinds.push(step(f, q, s)?);
    }
    // The tail: [guard ->] [DoorOfTime Complete ->] transition (last, no next).
    let mut go: Kind = f.events[later].kind.clone();
    set_next(&mut go, NONE)?;
    let t: u16 = chain(f, DOOR_PREFIX, vec![go], NONE)?;
    let rec: usize = find(f, RECOVERY)?;
    if !matches!(&f.events[rec].kind, Kind::Sub { entry, .. } if entry == "Recovery") {
        return Err(Error::Format("story skip: Event102 is not the Recovery call, the game files differ"));
    }
    let after: u16 = next_of(&f.events[rec].kind).ok_or(Error::Format("story skip: the Recovery call has no next"))?;
    let is_complete = |f: &Flow, i: usize| text(&f.events[i], "ChallengeName") == Some("DoorOfTime") && text(&f.events[i], "StepName") == Some("Complete");
    let tail: u16 = match f.events.get(after as usize).map(|e| e.kind.clone()) {
        // door_fix's guard: case 0 (step06 or later) asks for Complete, the other case ends.
        Some(Kind::Switch { actor, query, cases, params }) => {
            let yes: u16 = cases.iter().find(|c| c.val == 0).map(|c| c.ev).ok_or(Error::Format("story skip: the quest guard has no true case"))?;
            if !is_complete(f, yes as usize) {
                return Err(Error::Format("story skip: the quest guard does not lead to the DoorOfTime Complete request"));
            }
            let mut done: Kind = f.events[yes as usize].kind.clone();
            set_next(&mut done, t)?;
            let c: u16 = chain(f, DOOR_PREFIX, vec![done], t)?;
            let cases = cases.iter().map(|x| crate::bfevfl::Case { val: x.val, ev: if x.val == 0 { c } else { t } }).collect();
            let g: u16 = idx(f.events.len())?;
            f.events.push(Event { name: format!("{DOOR_PREFIX}{g}"), kind: Kind::Switch { actor, query, cases, params } });
            g
        }
        Some(Kind::Action { .. }) if is_complete(f, after as usize) => {
            let mut done: Kind = f.events[after as usize].kind.clone();
            set_next(&mut done, t)?;
            chain(f, DOOR_PREFIX, vec![done], t)?
        }
        _ => return Err(Error::Format("story skip: Recovery is not followed by the DoorOfTime Complete request")),
    };
    let first: u16 = chain(f, DOOR_PREFIX, kinds, tail)?;
    let Kind::Switch { cases, .. } = &mut f.events[sw].kind else { unreachable!() };
    let c = cases.iter_mut().find(|c| c.val == 1).ok_or(Error::Format("story skip: the draw check has no false case"))?;
    c.ev = first;
    Ok(())
}

// ---------------------------------------------------------------- staging

fn source(rf: &mut Romfs, have: Have, path: &str) -> Res<Vec<u8>> {
    let z: Vec<u8> = match have(path) {
        Some(b) => b,
        None => rf.read(path)?,
    };
    rf.zs.decompress(&z)
}

/// The item the location patcher left in a grant event (the vanilla one when unshuffled).
pub fn placed(rf: &mut Romfs, have: Have, file: &str, index: usize, event: &str, param: &str) -> Res<String> {
    let raw: Vec<u8> = source(rf, have, file)?;
    param_of(&raw, index, event, param)
}

/// Rewrites the bridge and the draw. `have` returns a file already staged by an earlier step (the door fix,
/// the location patcher), so those edits are kept and Rauru's gift follows the shuffled item.
pub fn stage(rf: &mut Romfs, have: Have) -> Res<Vec<Out>> {
    let medal: String = placed(rf, have, RAURU_FILE, RAURU_INDEX, RAURU_EVENT, RAURU_PARAM)?;
    let l: crate::flags::FlagList = crate::flags_build::load(rf)?;
    let got: String = format!("IsGet.{medal}");
    let got: Option<&str> = l.by_hash(crate::flags::hash_name(&got) as u64).is_some().then_some(got.as_str());
    Ok(vec![
        edit_zs(rf, have, BRIDGE_FILE, |raw| rebuild(raw, patch_bridge))?,
        edit_zs(rf, have, GIFT_FILE, |raw| rebuild(raw, patch_gift))?,
        edit_zs(rf, have, DOOR_FILE, |raw| rebuild(raw, |b| patch_draw(b, &medal, got)))?,
    ])
}

pub fn worked(rf: &mut Romfs, pk: &crate::modpack::Pack) -> Res<Vec<Out>> {
    let root: std::path::PathBuf = pk.work.join("romfs");
    let have = |p: &str| -> Option<Vec<u8>> { fs::read(p.split('/').fold(root.clone(), |a, s| a.join(s))).ok() };
    stage(rf, &have)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump() -> Option<std::path::PathBuf> {
        let p: std::path::PathBuf = std::env::var_os("OOTRR_ROMFS").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from("D:/Oot/romfs"));
        if p.is_dir() {
            Some(p)
        } else {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", p.display());
            None
        }
    }

    fn none(_p: &str) -> Option<Vec<u8>> {
        None
    }

    /// Follows single nexts from `start`, collecting each event's summary.
    fn walk(f: &Flow, start: u16, limit: usize) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut i: u16 = start;
        while i != NONE && out.len() < limit {
            let e: &Event = &f.events[i as usize];
            let what: String = match &e.kind {
                Kind::Action { .. } => {
                    let a: String = action(f, i as usize).unwrap_or_default();
                    let p: String = ["EventName", "GameDataName", "Actor", "ChallengeName", "StepName"].iter().filter_map(|k| text(e, k)).collect::<Vec<_>>().join(",");
                    format!("{a}({p})")
                }
                Kind::Sub { flow, entry, .. } => format!("sub {flow}:{entry}"),
                k => format!("{k:?}"),
            };
            out.push(what);
            i = next_of(&e.kind).unwrap_or(NONE);
        }
        out
    }

    #[test]
    fn the_new_game_state_covers_the_deku_tree_and_the_child_temple() {
        let o: Ops = deku();
        assert_eq!(o.steps[0], ("Meet_DekuTree".to_string(), "Complete".to_string()));
        assert_eq!(o.steps.last().unwrap(), &("Get_StoneOfForest".to_string(), "MeetBoss".to_string()));
        assert!(o.flags.iter().any(|(n, _)| n == "KokiriForest_Meet_DekuTree_OpenDekuTreeMouth"));
        assert!(o.items.is_empty() && o.time.is_none());
        let s: Ops = start();
        assert_eq!(s.flags.len(), 4);
        assert!(s.flags.iter().all(|(_, v)| *v));
        assert!(!s.flags.iter().any(|(n, _)| n.contains("Kokiri")));
        assert!(s.steps.is_empty() && s.items.is_empty());
    }

    #[test]
    fn the_real_draw_hands_over_to_the_later_draw() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        assert_eq!(placed(&mut rf, &none, RAURU_FILE, RAURU_INDEX, RAURU_EVENT, RAURU_PARAM).unwrap(), "Item_Medal_Light");
        let mut b: Bf = Bf::parse(&rf.read_zs(DOOR_FILE).unwrap()).unwrap();
        crate::door_fix::patch_flow(&mut b).unwrap();
        patch_draw(&mut b, "Item_Test", Some("IsGet.Item_Test")).unwrap();
        assert!(patch_draw(&mut b, "Item_Test", None).is_err());
        let f: &Flow = b.flows.iter().find(|f| f.name == DOOR_FLOW).unwrap();
        let Kind::Switch { cases, .. } = &f.events[find(f, DRAW_SWITCH).unwrap()].kind else { panic!() };
        let later: u16 = cases.iter().find(|c| c.val == 0).unwrap().ev;
        assert_eq!(walk(f, later, 2), vec!["EventSystemActor.EventTransition(Market_Child_Field_ChildToAdult)".to_string()]);
        let w: Vec<String> = walk(f, cases.iter().find(|c| c.val == 1).unwrap().ev, 30);
        assert!(w[1].contains("HavePlayedEvent.Dm_TimeTemple_0040"));
        assert_eq!(w[9], "PouchSystemActor.EventTriggerAddOrRemovePouchContent(Weapon_Sword_Master)");
        assert_eq!(w[10], "PouchSystemActor.EventTriggerAddOrRemovePouchContent(Item_Test)");
        assert_eq!(w[11], "GameDataActor.EventTriggerSetGameDataBool(IsGet.Item_Test)");
        assert_eq!(w[12], "ChallengeSystemActor.EventExecuteProgressChallengeStep(SageOfLight,Complete)");
        assert_eq!(w[14], "ChallengeSystemActor.EventExecuteProgressChallengeStep(SageOfForest,Step01)");
        assert!(w[15].starts_with("Switch"), "{}", w[15]);
        assert_eq!(w.len(), 16);
        // The guard: Step06 or later asks for Complete then transitions, else just transitions;
        // the transition is the last event either way.
        let mut i: u16 = cases.iter().find(|c| c.val == 1).unwrap().ev;
        for _ in 0..15 {
            i = next_of(&f.events[i as usize].kind).unwrap();
        }
        let Kind::Switch { cases: g, .. } = &f.events[i as usize].kind else { panic!() };
        let yes: Vec<String> = walk(f, g.iter().find(|c| c.val == 0).unwrap().ev, 5);
        assert_eq!(yes, vec!["ChallengeSystemActor.EventExecuteProgressChallengeStep(DoorOfTime,Complete)".to_string(), "EventSystemActor.EventTransition(Market_Child_Field_ChildToAdult)".to_string()]);
        let no: Vec<String> = walk(f, g.iter().find(|c| c.val != 0).unwrap().ev, 5);
        assert_eq!(no, vec!["EventSystemActor.EventTransition(Market_Child_Field_ChildToAdult)".to_string()]);
        assert!(!w.iter().chain(&yes).any(|s| s.starts_with("sub")));
        assert!(f.events.iter().filter(|e| e.name.starts_with(DOOR_PREFIX)).all(|e| match &e.kind {
            Kind::Action { params, .. } => params.as_ref().and_then(|c| c.get("IsEquipDIrect")) != Some(&Val::Bool(TRUE)),
            _ => true,
        }));
        Bf::parse(&b.write().unwrap()).unwrap();
        assert_eq!(stage(&mut rf, &none).unwrap().len(), 2);
    }

    #[test]
    fn the_real_bridge_goes_straight_to_the_gift_scene() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(BRIDGE_FILE).unwrap()).unwrap();
        crate::start_state::keep_field_clock(&mut b).unwrap();
        patch_bridge(&mut b).unwrap();
        assert!(patch_bridge(&mut b).is_err());
        let f: &Flow = b.flows.iter().find(|f| f.name == BRIDGE_FLOW).unwrap();
        let w: Vec<String> = walk(f, find(f, DUCKING).unwrap() as u16, 6);
        assert!(w[0].starts_with("SoundSystemActor.EventSoundRequestDucking"));
        assert_eq!(w[1], "GameDataActor.EventTriggerSetGameDataBool(HavePlayedEvent.Dm_Kokiri_0150)");
        assert!(w[2].starts_with("SoundSystemActor.EventSoundReleaseDucking"));
        assert_eq!(w[3], "EventSystemActor.EventTransition(Dm_Kokiri_0155)");
        assert_eq!(w[4], "GameDataActor.EventTriggerSetGameDataBool(HyruleField_ChangeENV)");
        assert!(w[5].contains("EventReserveBancChange"));
        Bf::parse(&b.write().unwrap()).unwrap();
    }

    #[test]
    fn the_real_gift_scene_is_a_plain_item_get() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(GIFT_FILE).unwrap()).unwrap();
        patch_gift(&mut b).unwrap();
        assert!(patch_gift(&mut b).is_err());
        let f: &Flow = b.flows.iter().find(|f| f.name == GIFT_FLOW).unwrap();
        let e = f.entries.iter().find(|e| e.name == GIFT_ENTRY).unwrap();
        let w: Vec<String> = walk(f, e.main, 5);
        assert_eq!(w, vec!["PouchSystemActor.EventTriggerAddOrRemovePouchContent(Item_Ocarina_Fairy)".to_string(), "sub GetPouchContent:DisplayGetByActorName".to_string()]);
        assert_eq!(e.subs, vec![find(f, GIFT_SHOW).unwrap() as u16]);
        let out: Vec<u8> = b.write().unwrap();
        assert_eq!(Bf::parse(&out).unwrap(), b);
    }
}
