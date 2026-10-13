use super::*;
use std::sync::atomic::Ordering::Relaxed;

#[test]
fn custom_race() {
    let mut race = new_race();
    assert_eq!(race_title(&race, &race.input), "February Stakes");
    assert_eq!(race.input.horses[0].skills[0].skill_id, 100521);
    let run = run_once(&race, &race.input);
    assert_eq!(verify_against_server(&race, &run).as_deref(), Some("no server replay"));
    assert_eq!(run.horses[0].place_by_progress, vec![1; 101]);
    let cs = cards();
    assert!(cs.len() > 100 && cs.windows(2).all(|w| (&w[0].name, w[0].card_id) < (&w[1].name, w[1].card_id)));
    let c = card_info(105202).unwrap();
    assert_eq!((c.chara_id, &*c.name, c.unique_skill), (1052, "Haru Urara", 110521));
    let mut h = new_horse(&race, 105201);
    assert_eq!((h.chara_id, h.running_style, h.proper_ground), (1052, 3, [1, 7]));
    h.skills.insert(0, SkillInput { skill_id: 200331, level: 1 });
    h.skills[1].level = 5;
    set_card(&mut h, 105202);
    assert_eq!((h.card_id, h.skills.clone()), (105202, vec![SkillInput { skill_id: 200331, level: 1 }, SkillInput { skill_id: 110521, level: 5 }]));
    // Already owning the new card's unique: the old one goes, nothing is duplicated.
    h.skills.push(SkillInput { skill_id: 100521, level: 3 });
    set_card(&mut h, 105201);
    assert_eq!(h.skills, vec![SkillInput { skill_id: 200331, level: 1 }, SkillInput { skill_id: 100521, level: 3 }]);
    for n in [2, 18] {
        race.input.horses = cs.iter().cycle().take(n).map(|c| new_horse(&race, c.card_id)).collect();
        let run = run_once(&race, &race.input);
        assert_eq!(run.horses.len(), n);
        assert!(run.horses.iter().all(|h| h.finish_time > 60.0 && h.finish_time < 200.0)); // default course: Tokyo dirt 1600m (~96 s)
    }
    let mut pop: Vec<i32> = custom_popularity(&race.input).horses.iter().map(|h| h.popularity).collect();
    pop.sort();
    assert_eq!(pop, (1..=18).collect::<Vec<_>>());
    // Off-course instance: generic title; set_course picks a G1 back.
    let mut input = race.input.clone();
    input.course_set_id = 10101;
    assert!(race_title(&race, &input).ends_with("m"));
    set_course(&race, &mut input, 10611);
    assert_eq!(race_title(&race, &input), "February Stakes");

    let cs = courses(&race);
    assert!(cs.len() > 50 && cs.iter().any(|c| c.id == 10606));
    let v = course_view(&race, 10606);
    assert_eq!((v.distance, v.final_leg_start), (2400.0, 1600.0));
    assert_eq!(v.phases, [0.0, 400.0, 1600.0, 2000.0]);
    let inside = |a: f32, b: f32| 0.0 <= a && a < b && b <= v.distance;
    assert!(!v.corners.is_empty() && v.corners.iter().chain(&v.straights).all(|s| inside(s.start, s.end)));
    assert!(!v.slopes.is_empty() && v.slopes.iter().all(|s| inside(s.start, s.end) && s.grade != 0.0));
    assert_eq!((v.elevation.len(), v.elevation[0]), (481, 0.0));
    // Tokyo: the climb on the home straight.
    assert!(v.elevation.iter().copied().fold(f32::MIN, f32::max) - v.elevation.iter().copied().fold(f32::MAX, f32::min) > 1.0);

    let w = Work::default();
    let cands = [200331, 100521, 201352]; // Professor of Curvature, Urara's unique (owned), Hydrate
    let g = skill_gains(&race, &new_race().input, 0, &cands, 8, &w).0;
    assert_eq!(w.done.load(Relaxed), 24);
    // Muted baseline: a skill that never activates changes nothing.
    assert!(g.iter().filter(|x| x.rate == 0.0).all(|x| x.samples.iter().all(|&v| v == 0.0)));
    assert!(g.len() == 3 && g.iter().all(|x| x.samples.len() == 8 && (0.0..=1.0).contains(&x.rate)));
    assert!(g.windows(2).all(|w| w[0].lengths.mean >= w[1].lengths.mean));
    w.cancel.store(true, Relaxed);
    assert!(skill_gains(&race, &race.input, 0, &cands, 8, &w).0.is_empty());

    // NPC lobby: the measured uma at gate 5 among 8 NPCs of distinct charas; the front-heavy end has 3 Runaways.
    let (f, i) = npc_field(&race, &new_race().input, 0, true, LOBBIES.len() - 1);
    assert_eq!((f.horses.len(), i, f.horses[i].card_id), (FIELD_MIN, 4, 105201));
    assert_eq!(f.horses.iter().filter(|h| runaway(&race, h)).count(), 3);
    assert_eq!(f.horses.iter().map(|h| h.chara_id).collect::<std::collections::HashSet<_>>().len(), FIELD_MIN);
    // Top-up: a 9-uma field stays as is, a 2-uma one gets 7 NPCs after it.
    let ids = |v: &[HorseInput]| v.iter().map(|h| h.card_id).collect::<Vec<_>>();
    assert_eq!(ids(&npc_field(&race, &f, 2, false, 2).0.horses), ids(&f.horses));
    let two = RaceInput { horses: f.horses[3..5].to_vec(), ..f.clone() };
    let (t, j) = npc_field(&race, &two, 1, false, 2);
    assert_eq!((t.horses.len(), j, ids(&t.horses[..2])), (FIELD_MIN, 1, ids(&two.horses)));
    // NPCs keep a full spurt on the longest course.
    let mut long = new_race().input;
    set_course(&race, &mut long, cs_long(&race));
    let (f, _) = npc_field(&race, &long, 0, true, 2);
    for s in seeds(1, 4) {
        let run = run_once(&race, &RaceInput { random_seed: s, ..f.clone() });
        assert!(f.horses.iter().zip(&run.horses).all(|(h, r)| h.viewer_id != 0 || r.full_spurt), "seed {s}");
        // HP along the course ends at the goal HP; a full spurt needs some HP, less than the start's.
        assert!(run.horses.iter().all(|r| (r.hp_by_progress[100] - r.hp_at_goal).abs() < 1.0 && r.spurt_need_hp > 0.0 && r.spurt_need_hp < r.hp_by_progress[0]));
    }
}

/// The longest turf course.
fn cs_long(race: &Race) -> i32 {
    courses(race).iter().filter(|c| c.surface == 1).max_by_key(|c| c.distance).unwrap().id
}

#[test]
fn skill_filters() {
    let race = new_race();
    let all = all_skills(&race);
    let get = |id: i32| all.iter().find(|s| s.id == id).unwrap().clone();
    let lead = get(200531); // Taking the Lead: running_style==1&phase==0, accel
    assert_eq!((lead.effects.clone(), lead.tags.clone()), (vec!["Acceleration"], vec!["Front", "Early race"]));
    assert_eq!(get(201352).effects, vec!["Recovery"]); // Hydrate
    assert_eq!(get(200851).effects, vec!["Debuff"]); // Hesitant Front Runners
    assert_eq!(get(200771).effects, vec!["Debuff", "Stamina drain"]); // Trick (Front)
    assert_eq!(get(200431).effects, vec!["Start"]); // Concentration
    // phase==1&corner==0
    assert_eq!(all.iter().find(|s| s.name == "Breath of Fresh Air").unwrap().tags, vec!["Mid race", "Straight"]);
    let (u, inh) = (get(100521), get(900521));
    assert_eq!((u.owner_card, u.inherited, inh.owner_card, inh.inherited), (Some(105201), false, Some(105201), true));
    assert_eq!(skill_info(200531), lead);
    assert!(all.iter().all(|s| s.effects.iter().all(|e| SKILL_EFFECTS.contains(e)) && s.tags.iter().all(|t| SKILL_TAGS.contains(t))));
    assert_eq!(skill_info(10071).name, "Warning Shot!");
    assert_eq!(skill_info(-1).name, "Skill -1");
}

/// Every listed course runs a 1-horse race.
#[test]
#[ignore]
fn all_courses() {
    let race = new_race();
    for c in courses(&race) {
        let mut input = race.input.clone();
        set_course(&race, &mut input, c.id);
        let r = run_once(&race, &input);
        let v = course_view(&race, c.id);
        assert!(r.horses[0].finish_time > 0.0 && v.elevation.len() > 1, "{c:?}");
        println!("{} {} {} {}m {} corners {} straights {} slopes, elevation {:.1}..{:.1} m", c.id, race_title(&race, &input), c.track, c.distance, v.corners.len(), v.straights.len(), v.slopes.len(),
            v.elevation.iter().copied().fold(f32::MAX, f32::min), v.elevation.iter().copied().fold(f32::MIN, f32::max));
    }
}

#[test]
fn horseact_lookup() {
    assert_eq!(expand_env("%USERPROFILE%\\Documents"), format!("{}\\Documents", std::env::var("USERPROFILE").unwrap()));
    assert_eq!(expand_env("a%NO_SUCH_VAR_X%b"), "a%NO_SUCH_VAR_X%b");
}

#[test]
fn skill_outcomes() {
    assert_eq!(term_text("running_style==1"), "Front");
    assert_eq!(term_text("phase>=2"), "Late race or later");
    assert_eq!(term_text("running_style_count_oikomi_otherself>=1"), "Other End ≥1");
    assert_eq!(term_text("order_rate<=50"), "Order 1st-5th");
    assert_eq!(term_text("mystery_key==3"), "mystery_key==3");
    // Bounds on one keyword merge; order rates become places in a 9-uma field (the sim's rounding).
    let g = condition_groups("is_finalcorner==1&order_rate>=40&order_rate<=75&is_overtake==1@phase==3&order>1");
    assert_eq!(g, ["Final corner or later + order 4th-7th + overtaking", "Last spurt + order 2nd-9th"]);
    assert_eq!(term_text("remain_distance<=401&remain_distance>=399"), "399-401m left");
}

#[test]
fn veterans() {
    let row = r#"[{"trained_chara_id":7,"card_id":105201,"speed":1148,"stamina":600,"power":800,"guts":400,"wiz":900,
        "running_style":3,"proper_distance_short":5,"proper_distance_mile":6,"proper_distance_middle":4,"proper_distance_long":2,
        "proper_running_style_nige":1,"proper_running_style_senko":3,"proper_running_style_sashi":7,"proper_running_style_oikomi":6,
        "proper_ground_turf":1,"proper_ground_dirt":8,"skill_array":[{"skill_id":100521,"level":3}],"rarity":3,"talent_level":3,
        "rank":14,"rank_score":13445,"fans":165394,"wins":12,"create_time":"2026-10-11 17:13:40","extra":null}]"#;
    let v = &parse_veterans(row.as_bytes()).unwrap()[0];
    assert_eq!((v.id, v.rating, v.horse.chara_id, v.horse.pow, v.horse.proper_ground, v.horse.final_grade), (7, 13445, 1052, 800, [1, 8], 14));
    assert_eq!(v.horse.skills, vec![SkillInput { skill_id: 100521, level: 3 }]);
    let labels: Vec<String> = [1, 2, 14, 18, 19, 20, 28, 29, 89].map(rank_label).into();
    assert_eq!(labels, ["G", "G+", "A+", "SS+", "UG", "UG1", "UG9", "UF", "US"]);
    let stats: Vec<String> = [0, 399, 400, 1099, 1100, 1200, 1201, 1234, 1999].map(|x| rank_label(stat_rank(x))).into();
    assert_eq!(stats, ["G", "D+", "C", "S", "SS", "SS+", "UG", "UG3", "US9"]);
}

#[test]
fn cm_from_save() {
    let id = |course, date| cm_of_save(course, date).map(|p| p.id);
    // Taurus Cup ran twice on Tokyo 2400m (CM 1 and CM 13): the save date picks the round.
    assert_eq!((id(10606, "2025-09-01"), id(10606, "2026-05-20")), (Some(1), Some(13)));
    assert_eq!((id(11103, "2026-08-14"), id(10606, "2025-01-01")), (Some(17), None));
}

#[test]
fn change_uma() {
    let race = new_race();
    let cs = cards();
    let a = &cs[0];
    let b = cs.iter().find(|c| c.chara_id != a.chara_id).unwrap();
    let mut h = new_horse(&race, a.card_id);
    h.proper_ground = [1, 1];
    h.speed = 1500;
    // A costume of the same uma keeps edited aptitudes; another uma brings its own. Stats stay either way.
    if let Some(a2) = cs.iter().find(|c| c.chara_id == a.chara_id && c.card_id != a.card_id) {
        set_card(&mut h, a2.card_id);
        assert_eq!(h.proper_ground, [1, 1]);
    }
    set_card(&mut h, b.card_id);
    let fresh = new_horse(&race, b.card_id);
    assert_eq!((h.chara_id, h.proper_ground, h.proper_distance, h.running_style), (b.chara_id, fresh.proper_ground, fresh.proper_distance, fresh.running_style));
    assert_eq!(h.speed, 1500);
}

#[test]
fn blob_sha_matches_git() {
    // `printf 'hello\n' | git hash-object --stdin`
    assert_eq!(blob_sha(b"hello\n"), "ce013625030ba8dba906f756967f9e9ca394464a");
}

#[test]
fn stamina_debuffs() {
    let race = new_race();
    let mut input = race.input.clone();
    // Urara (Late Surger) and an uma with Subdued Late Surgers (drains a Late Surger's HP early in the race).
    let mut owner = new_horse(&race, cards()[0].card_id);
    owner.skills.push(SkillInput { skill_id: 200891, level: 1 });
    input.horses = vec![new_horse(&race, 105201), owner];
    assert!(drains_hp(&race, 200891) && !drains_hp(&race, 200331));
    let st = stamina(&race, &input, 0, &seeds(1, 8), true, &Work::default());
    assert_eq!((st.loss.len(), st.stats.times.len(), st.debuffs.len()), (8, 8, 1));
    let d = &st.debuffs[0];
    assert_eq!((d.owner, d.skill_id), (1, 200891));
    // Hits only when it activates; when it hits it takes HP, once per hit.
    assert!(d.hit_rate > 0.0 && d.hit_rate <= d.rate && d.hp_lost > 0.0 && d.hit_at.len() == (d.hit_rate * 8.0).round() as usize);
    assert!(st.all_muted.is_some() && (0.0..=1.0).contains(&st.full_at_max));
    // Low stamina on the longest course: it costs lengths, and max stamina spurts more often.
    set_course(&race, &mut input, cs_long(&race));
    input.horses[0].stamina = 400;
    let st = stamina(&race, &input, 0, &seeds(1, 8), true, &Work::default());
    // Per seed it's noisy (the runs diverge, and with them later RNG draws); on average it costs.
    assert!(st.loss.iter().map(|x| x.0).sum::<f32>() < -0.5 && st.full_at_max > st.stats.full_spurt_rate);
}

#[test]
fn wit_mechanics() {
    let race = new_race();
    let mut input = race.input.clone();
    input.horses = vec![new_horse(&race, 105201), new_horse(&race, cards()[0].card_id)];
    input.horses[0].wiz = 1200;
    // The same Wit as the old one: nothing changes.
    let w = wit(&race, &input, 0, 1200, &seeds(1, 4), &Work::default());
    assert_eq!(w.rows.len(), WIT_MECHANICS.len() + 1);
    assert!(w.rows.iter().all(|r| r.len() == 4) && w.rows.iter().flatten().all(|p| p.gain == 0.0 && p.new == p.old));
    // 1200 vs 200: the section rolls move every race, and all at once costs the old Wit lengths on average.
    let w = wit(&race, &input, 0, 200, &seeds(1, 16), &Work::default());
    assert!(w.rows[0].iter().all(|p| p.gain != 0.0));
    assert!(w.rows[WIT_MECHANICS.len()].iter().map(|p| p.gain).sum::<f32>() > 0.0);
    // Every row starts from the same baseline race.
    assert!(w.rows.iter().all(|r| r.iter().zip(&w.rows[0]).all(|(p, q)| p.old == q.old)));
}
