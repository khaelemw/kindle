//! Enemies by name, for `kindle.world.spawn{enemy = "black_knight"}`: each kind's model,
//! NpcParam, think param (its AI) and home area (the map whose AI scripts it needs). One entry
//! per kind (its first placement in the game's maps), hostile enemies only, no human (c0000)
//! characters. The ids are the game's own, from its map placements, as listed by the DS3
//! randomizer's enemy list (generated from the game's data by a script).

pub struct Enemy {
    /// snake_case name, the Lua key.
    pub key: &'static str,
    pub name: &'static str,
    /// Model number: 1300 for c1300.
    pub model: u32,
    pub npc_param: i32,
    pub think_param: i32,
    /// The map whose AI scripts it needs ("m33_00_00_00").
    pub area: &'static str,
}

pub const ENEMIES: &[Enemy] = &[
    Enemy { key: "abyss_watchers", name: "Abyss Watchers", model: 3040, npc_param: 304000, think_param: 304005, area: "m33_00_00_00" },
    Enemy { key: "aldrich_devourer_of_gods", name: "Aldrich, Devourer of Gods", model: 5150, npc_param: 515000, think_param: 515000, area: "m37_00_00_00" },
    Enemy { key: "ancient_wyvern", name: "Ancient Wyvern", model: 3141, npc_param: 314110, think_param: 314100, area: "m30_00_00_00" },
    Enemy { key: "ancient_wyvern_boss", name: "Ancient Wyvern Boss", model: 3141, npc_param: 314100, think_param: 314000, area: "m32_00_00_00" },
    Enemy { key: "ancient_wyvern_mob", name: "Ancient Wyvern Mob", model: 3140, npc_param: 314000, think_param: 314010, area: "m32_00_00_00" },
    Enemy { key: "angel", name: "Angel", model: 6250, npc_param: 625000, think_param: 625000, area: "m50_00_00_00" },
    Enemy { key: "angel_pilgrim", name: "Angel Pilgrim", model: 6240, npc_param: 624000, think_param: 624000, area: "m50_00_00_00" },
    Enemy { key: "anor_londo_writhing_flesh", name: "Anor Londo Writhing Flesh", model: 2131, npc_param: 213110, think_param: 213000, area: "m35_00_00_00" },
    Enemy { key: "archdeacon_of_the_deep", name: "Archdeacon of the Deep", model: 5220, npc_param: 522000, think_param: 522000, area: "m35_00_00_00" },
    Enemy { key: "archives_gargoyle", name: "Archives Gargoyle", model: 2190, npc_param: 219000, think_param: 219000, area: "m34_01_00_00" },
    Enemy { key: "ariandel_greater_crab", name: "Ariandel Greater Crab", model: 6130, npc_param: 613000, think_param: 613000, area: "m45_00_00_00" },
    Enemy { key: "basilisk", name: "Basilisk", model: 2140, npc_param: 214100, think_param: 214001, area: "m33_00_00_00" },
    Enemy { key: "black_knight", name: "Black Knight", model: 1300, npc_param: 130610, think_param: 130010, area: "m33_00_00_00" },
    Enemy { key: "bonewheel_skeleton", name: "Bonewheel Skeleton", model: 1470, npc_param: 147000, think_param: 147000, area: "m38_00_00_00" },
    Enemy { key: "boreal_outrider_knight", name: "Boreal Outrider Knight", model: 1310, npc_param: 131200, think_param: 131000, area: "m30_01_00_00" },
    Enemy { key: "bridge_darkeater_midir", name: "Bridge Darkeater Midir", model: 6210, npc_param: 621000, think_param: 621000, area: "m51_00_00_00" },
    Enemy { key: "cage_spider", name: "Cage Spider", model: 3090, npc_param: 309000, think_param: 309000, area: "m31_00_00_00" },
    Enemy {
        key: "carthus_curved_sword_skeleton",
        name: "Carthus Curved Sword Skeleton",
        model: 1170,
        npc_param: 117000,
        think_param: 117001,
        area: "m38_00_00_00",
    },
    Enemy { key: "carthus_sandworm", name: "Carthus Sandworm", model: 2200, npc_param: 220000, think_param: 220000, area: "m38_00_00_00" },
    Enemy { key: "carthus_shotel_skeleton", name: "Carthus Shotel Skeleton", model: 1180, npc_param: 118000, think_param: 118000, area: "m38_00_00_00" },
    Enemy { key: "catacombs_writhing_flesh", name: "Catacombs Writhing Flesh", model: 2130, npc_param: 213000, think_param: 213000, area: "m38_00_00_00" },
    Enemy { key: "cathedral_deacon", name: "Cathedral Deacon", model: 5223, npc_param: 522301, think_param: 522300, area: "m35_00_00_00" },
    Enemy { key: "cathedral_evangelist", name: "Cathedral Evangelist", model: 1230, npc_param: 123000, think_param: 123001, area: "m31_00_00_00" },
    Enemy { key: "cathedral_hollow_soldier", name: "Cathedral Hollow Soldier", model: 1440, npc_param: 144040, think_param: 144040, area: "m35_00_00_00" },
    Enemy { key: "cathedral_knight", name: "Cathedral Knight", model: 1190, npc_param: 119111, think_param: 119010, area: "m30_00_00_00" },
    Enemy {
        key: "cathedral_large_hollow_soldier",
        name: "Cathedral Large Hollow Soldier",
        model: 1445,
        npc_param: 144510,
        think_param: 110510,
        area: "m35_00_00_00",
    },
    Enemy { key: "cemetery_hollow", name: "Cemetery Hollow", model: 1430, npc_param: 143040, think_param: 143041, area: "m40_00_00_00" },
    Enemy { key: "consumed_king_oceiros", name: "Consumed King Oceiros", model: 2090, npc_param: 209000, think_param: 209000, area: "m30_00_00_00" },
    Enemy { key: "corpse_grub", name: "Corpse-grub", model: 2180, npc_param: 218110, think_param: 218010, area: "m34_01_00_00" },
    Enemy { key: "corvian", name: "Corvian", model: 2210, npc_param: 221020, think_param: 221020, area: "m33_00_00_00" },
    Enemy { key: "corvian_knight", name: "Corvian Knight", model: 6070, npc_param: 607010, think_param: 607010, area: "m45_00_00_00" },
    Enemy { key: "corvian_settler", name: "Corvian Settler", model: 6080, npc_param: 608010, think_param: 608000, area: "m45_00_00_00" },
    Enemy { key: "crystal_lizard", name: "Crystal Lizard", model: 2150, npc_param: 215000, think_param: 215000, area: "m30_00_00_00" },
    Enemy { key: "crystal_sage_in_archives", name: "Crystal Sage in Archives", model: 1321, npc_param: 132100, think_param: 132050, area: "m34_01_00_00" },
    Enemy { key: "crystal_sage_in_farron", name: "Crystal Sage in Farron", model: 1320, npc_param: 132000, think_param: 132000, area: "m33_00_00_00" },
    Enemy {
        key: "dancer_of_the_boreal_valley",
        name: "Dancer of the Boreal Valley",
        model: 5270,
        npc_param: 527000,
        think_param: 527000,
        area: "m30_00_00_00",
    },
    Enemy { key: "darkeater_midir", name: "Darkeater Midir", model: 6211, npc_param: 621100, think_param: 621100, area: "m51_00_00_00" },
    Enemy { key: "darkwraith", name: "Darkwraith", model: 3170, npc_param: 317000, think_param: 317000, area: "m30_00_00_00" },
    Enemy { key: "deep_accursed", name: "Deep Accursed", model: 3110, npc_param: 311100, think_param: 311000, area: "m35_00_00_00" },
    Enemy { key: "demon", name: "Demon", model: 3060, npc_param: 306000, think_param: 306000, area: "m31_00_00_00" },
    Enemy { key: "demon_cleric", name: "Demon Cleric", model: 3070, npc_param: 307000, think_param: 307000, area: "m38_00_00_00" },
    Enemy { key: "demon_cleric_helper", name: "Demon Cleric Helper", model: 3071, npc_param: 307100, think_param: 307100, area: "m38_00_00_00" },
    Enemy { key: "demon_prince", name: "Demon Prince", model: 5020, npc_param: 502000, think_param: 502000, area: "m50_00_00_00" },
    Enemy { key: "demon_prince_helper_1", name: "Demon Prince Helper 1", model: 5021, npc_param: 502100, think_param: 502100, area: "m50_00_00_00" },
    Enemy { key: "demon_prince_helper_2", name: "Demon Prince Helper 2", model: 5022, npc_param: 502200, think_param: 502200, area: "m50_00_00_00" },
    Enemy { key: "demonic_statue", name: "Demonic Statue", model: 3230, npc_param: 323000, think_param: 323000, area: "m38_00_00_00" },
    Enemy { key: "dragonslayer_armour", name: "Dragonslayer Armour", model: 3160, npc_param: 316000, think_param: 316000, area: "m30_01_00_00" },
    Enemy { key: "drakeblood_knight", name: "Drakeblood Knight", model: 1000, npc_param: 28800, think_param: 28800, area: "m32_00_00_00" },
    Enemy { key: "dreg_heap_hollow_slave", name: "Dreg Heap Hollow Slave", model: 1201, npc_param: 120521, think_param: 120121, area: "m50_00_00_00" },
    Enemy { key: "dreg_heap_lothric_knight", name: "Dreg Heap Lothric Knight", model: 1283, npc_param: 128312, think_param: 128311, area: "m50_00_00_00" },
    Enemy { key: "dummy", name: "Dummy", model: 100, npc_param: 0, think_param: 0, area: "m35_00_00_00" },
    Enemy { key: "elder_ghru", name: "Elder Ghru", model: 3120, npc_param: 312000, think_param: 312000, area: "m33_00_00_00" },
    Enemy { key: "farron_follower", name: "Farron Follower", model: 6000, npc_param: 600002, think_param: 600002, area: "m45_00_00_00" },
    Enemy { key: "father_ariandel", name: "Father Ariandel", model: 6010, npc_param: 601001, think_param: 601000, area: "m45_00_00_00" },
    Enemy { key: "fire_witch", name: "Fire Witch", model: 5240, npc_param: 524000, think_param: 524000, area: "m37_00_00_00" },
    Enemy { key: "ghru_grunt", name: "Ghru Grunt", model: 1210, npc_param: 121010, think_param: 121010, area: "m33_00_00_00" },
    Enemy { key: "giant_fly", name: "Giant Fly", model: 6090, npc_param: 609001, think_param: 609000, area: "m45_00_00_00" },
    Enemy { key: "giant_slave", name: "Giant Slave", model: 3021, npc_param: 302101, think_param: 302102, area: "m35_00_00_00" },
    Enemy {
        key: "grand_archives_hollow_soldier",
        name: "Grand Archives Hollow Soldier",
        model: 1102,
        npc_param: 110200,
        think_param: 110005,
        area: "m34_01_00_00",
    },
    Enemy { key: "grand_archives_scholar", name: "Grand Archives Scholar", model: 1340, npc_param: 134000, think_param: 134000, area: "m34_01_00_00" },
    Enemy { key: "grave_warden", name: "Grave Warden", model: 1250, npc_param: 125000, think_param: 125000, area: "m35_00_00_00" },
    Enemy { key: "greater_crab", name: "Greater Crab", model: 2270, npc_param: 227001, think_param: 227000, area: "m33_00_00_00" },
    Enemy { key: "greatwolf", name: "Greatwolf", model: 6030, npc_param: 603000, think_param: 603001, area: "m45_00_00_00" },
    Enemy { key: "gundyr", name: "Gundyr", model: 5110, npc_param: 511000, think_param: 511000, area: "m40_00_00_00" },
    Enemy { key: "harald_legion_knight", name: "Harald Legion Knight", model: 6320, npc_param: 632001, think_param: 632010, area: "m50_00_00_00" },
    Enemy { key: "havel_knight", name: "Havel Knight", model: 1000, npc_param: 28700, think_param: 28700, area: "m32_00_00_00" },
    Enemy { key: "high_lord_wolnir", name: "High Lord Wolnir", model: 5160, npc_param: 516000, think_param: 516000, area: "m38_00_00_00" },
    Enemy { key: "high_wall_lothric_knight", name: "High Wall Lothric Knight", model: 1280, npc_param: 128000, think_param: 128000, area: "m30_00_00_00" },
    Enemy { key: "hollow_cleric", name: "Hollow Cleric", model: 6290, npc_param: 629000, think_param: 629003, area: "m51_00_00_00" },
    Enemy { key: "hollow_manservant", name: "Hollow Manservant", model: 1260, npc_param: 126010, think_param: 126001, area: "m31_00_00_00" },
    Enemy { key: "hollow_slave", name: "Hollow Slave", model: 1200, npc_param: 120300, think_param: 120010, area: "m30_00_00_00" },
    Enemy { key: "hollow_soldier", name: "Hollow Soldier", model: 1100, npc_param: 110070, think_param: 110054, area: "m30_00_00_00" },
    Enemy { key: "hound_rat", name: "Hound Rat", model: 2110, npc_param: 211000, think_param: 211000, area: "m31_00_00_00" },
    Enemy { key: "humanity_sprite", name: "Humanity Sprite", model: 6232, npc_param: 623201, think_param: 623200, area: "m50_00_00_00" },
    Enemy { key: "infested_corpse", name: "Infested Corpse", model: 2060, npc_param: 206000, think_param: 206000, area: "m35_00_00_00" },
    Enemy { key: "invisible", name: "Invisible", model: 1000, npc_param: 0, think_param: 0, area: "m37_00_00_00" },
    Enemy { key: "irirthyll_deacon", name: "Irirthyll Deacon", model: 5227, npc_param: 522701, think_param: 522700, area: "m37_00_00_00" },
    Enemy {
        key: "irirthyll_dungeon_peasant_hollow",
        name: "Irirthyll Dungeon Peasant Hollow",
        model: 1241,
        npc_param: 124150,
        think_param: 124050,
        area: "m39_00_00_00",
    },
    Enemy { key: "irirthyll_tall_deacon", name: "Irirthyll Tall Deacon", model: 5226, npc_param: 522601, think_param: 522610, area: "m37_00_00_00" },
    Enemy { key: "irirthyll_wide_deacon", name: "Irirthyll Wide Deacon", model: 5225, npc_param: 522511, think_param: 522101, area: "m35_00_00_00" },
    Enemy { key: "irithyll_giant_slave", name: "Irithyll Giant Slave", model: 3020, npc_param: 302020, think_param: 302020, area: "m31_00_00_00" },
    Enemy { key: "irithyll_starved_hound", name: "Irithyll Starved Hound", model: 2290, npc_param: 229000, think_param: 202201, area: "m37_00_00_00" },
    Enemy { key: "irithyllian_slave", name: "Irithyllian Slave", model: 1350, npc_param: 135031, think_param: 135030, area: "m37_00_00_00" },
    Enemy { key: "jailer", name: "Jailer", model: 2230, npc_param: 223000, think_param: 223000, area: "m39_00_00_00" },
    Enemy { key: "judicator", name: "Judicator", model: 6280, npc_param: 628010, think_param: 628010, area: "m51_00_00_00" },
    Enemy { key: "judicator_helper", name: "Judicator Helper", model: 6281, npc_param: 628100, think_param: 628100, area: "m51_00_00_00" },
    Enemy { key: "king_of_the_storm", name: "King of the Storm", model: 5030, npc_param: 503000, think_param: 503000, area: "m32_00_00_00" },
    Enemy { key: "large_hollow_soldier", name: "Large Hollow Soldier", model: 1105, npc_param: 110510, think_param: 110510, area: "m30_00_00_00" },
    Enemy { key: "large_hound_rat", name: "Large Hound Rat", model: 2280, npc_param: 228200, think_param: 228000, area: "m31_00_00_00" },
    Enemy { key: "large_serpent_man", name: "Large Serpent-Man", model: 1391, npc_param: 139110, think_param: 139110, area: "m32_00_00_00" },
    Enemy { key: "large_starved_hound", name: "Large Starved Hound", model: 2020, npc_param: 202001, think_param: 202000, area: "m30_00_00_00" },
    Enemy { key: "lesser_crab", name: "Lesser Crab", model: 2271, npc_param: 227100, think_param: 227101, area: "m33_00_00_00" },
    Enemy { key: "locust_preacher", name: "Locust Preacher", model: 6330, npc_param: 633001, think_param: 633010, area: "m51_00_00_00" },
    Enemy { key: "lords_of_cinder", name: "Lords of Cinder", model: 5280, npc_param: 528000, think_param: 528000, area: "m41_00_00_00" },
    Enemy {
        key: "lothric_castle_hollow_soldier",
        name: "Lothric Castle Hollow Soldier",
        model: 1101,
        npc_param: 110140,
        think_param: 110025,
        area: "m30_01_00_00",
    },
    Enemy {
        key: "lothric_castle_large_hollow_soldier",
        name: "Lothric Castle Large Hollow Soldier",
        model: 1106,
        npc_param: 110600,
        think_param: 110500,
        area: "m30_01_00_00",
    },
    Enemy {
        key: "lothric_castle_lothric_knight",
        name: "Lothric Castle Lothric Knight",
        model: 1281,
        npc_param: 128110,
        think_param: 128010,
        area: "m30_01_00_00",
    },
    Enemy { key: "lothric_priest", name: "Lothric Priest", model: 1442, npc_param: 144300, think_param: 144070, area: "m30_00_00_00" },
    Enemy { key: "lothric_thief", name: "Lothric Thief", model: 6270, npc_param: 627043, think_param: 627000, area: "m51_00_00_00" },
    Enemy { key: "lycanthrope", name: "Lycanthrope", model: 1360, npc_param: 136001, think_param: 136001, area: "m33_00_00_00" },
    Enemy { key: "lycanthrope_hunter", name: "Lycanthrope Hunter", model: 1370, npc_param: 137001, think_param: 137000, area: "m33_00_00_00" },
    Enemy { key: "maggot_belly_starved_hound", name: "Maggot Belly Starved Hound", model: 2080, npc_param: 208000, think_param: 208000, area: "m33_00_00_00" },
    Enemy { key: "millwood_knight", name: "Millwood Knight", model: 6100, npc_param: 610000, think_param: 610000, area: "m45_00_00_00" },
    Enemy { key: "mimic_chest", name: "Mimic Chest", model: 2120, npc_param: 212500, think_param: 212000, area: "m30_00_00_00" },
    Enemy { key: "monstrosity_of_sin", name: "Monstrosity of Sin", model: 2040, npc_param: 204000, think_param: 204000, area: "m39_00_00_00" },
    Enemy { key: "murkman", name: "Murkman", model: 6231, npc_param: 623120, think_param: 623101, area: "m50_00_00_00" },
    Enemy { key: "murkman_summoner", name: "Murkman Summoner", model: 6230, npc_param: 623006, think_param: 623004, area: "m50_00_00_00" },
    Enemy { key: "nameless_king", name: "Nameless King", model: 5010, npc_param: 501000, think_param: 501000, area: "m32_00_00_00" },
    Enemy { key: "old_demon_king", name: "Old Demon King", model: 3050, npc_param: 305000, think_param: 305000, area: "m38_00_00_00" },
    Enemy { key: "peasant_hollow", name: "Peasant Hollow", model: 1240, npc_param: 124040, think_param: 124040, area: "m31_00_00_00" },
    Enemy { key: "pilgrim_butterfly", name: "Pilgrim Butterfly", model: 3080, npc_param: 308000, think_param: 308000, area: "m30_01_00_00" },
    Enemy { key: "poisonhorn_bug", name: "Poisonhorn Bug", model: 3210, npc_param: 321000, think_param: 321000, area: "m33_00_00_00" },
    Enemy { key: "pontiff_knight", name: "Pontiff Knight", model: 2030, npc_param: 203000, think_param: 203005, area: "m37_00_00_00" },
    Enemy { key: "pontiff_sulyvahn", name: "Pontiff Sulyvahn", model: 5140, npc_param: 514000, think_param: 514000, area: "m37_00_00_00" },
    Enemy { key: "profaned_capital_gargoyle", name: "Profaned Capital Gargoyle", model: 2191, npc_param: 219100, think_param: 219000, area: "m39_00_00_00" },
    Enemy { key: "pus_of_man", name: "Pus of Man", model: 1090, npc_param: 109000, think_param: 109000, area: "m30_00_00_00" },
    Enemy { key: "rapier_champion", name: "Rapier Champion", model: 1000, npc_param: 28900, think_param: 28900, area: "m32_00_00_00" },
    Enemy { key: "ravenous_crystal_lizard", name: "Ravenous Crystal Lizard", model: 3100, npc_param: 310400, think_param: 310000, area: "m33_00_00_00" },
    Enemy { key: "reanimated_corpse", name: "Reanimated Corpse", model: 1220, npc_param: 122000, think_param: 122020, area: "m35_00_00_00" },
    Enemy { key: "red_eyed_lothric_knight", name: "Red-Eyed Lothric Knight", model: 1282, npc_param: 128004, think_param: 128100, area: "m30_00_00_00" },
    Enemy { key: "ringed_city_skeleton", name: "Ringed City Skeleton", model: 1071, npc_param: 107301, think_param: 107001, area: "m51_00_00_00" },
    Enemy { key: "ringed_knight", name: "Ringed Knight", model: 6260, npc_param: 626020, think_param: 626020, area: "m51_00_00_00" },
    Enemy {
        key: "road_of_sacrifices_hollow_soldier",
        name: "Road of Sacrifices Hollow Soldier",
        model: 1441,
        npc_param: 144102,
        think_param: 110005,
        area: "m33_00_00_00",
    },
    Enemy {
        key: "road_of_sacrifices_sorcerer",
        name: "Road of Sacrifices Sorcerer",
        model: 1446,
        npc_param: 144620,
        think_param: 110520,
        area: "m33_00_00_00",
    },
    Enemy { key: "rock_lizard", name: "Rock Lizard", model: 3220, npc_param: 322000, think_param: 322001, area: "m32_00_00_00" },
    Enemy { key: "rotted_greatwood", name: "Rotted Greatwood", model: 5180, npc_param: 518000, think_param: 518000, area: "m31_00_00_00" },
    Enemy { key: "rotten_slug", name: "Rotten Slug", model: 1130, npc_param: 113000, think_param: 113000, area: "m30_00_00_00" },
    Enemy { key: "serpent_man", name: "Serpent-Man", model: 1390, npc_param: 139050, think_param: 139000, area: "m30_00_00_00" },
    Enemy { key: "serpent_man_summoner", name: "Serpent-Man Summoner", model: 1380, npc_param: 138000, think_param: 138000, area: "m32_00_00_00" },
    Enemy { key: "sewer_centipede", name: "Sewer Centipede", model: 2100, npc_param: 210000, think_param: 210000, area: "m37_00_00_00" },
    Enemy { key: "silver_knight", name: "Silver Knight", model: 1410, npc_param: 141020, think_param: 141020, area: "m37_00_00_00" },
    Enemy { key: "sister_friede", name: "Sister Friede", model: 6020, npc_param: 602001, think_param: 602001, area: "m45_00_00_00" },
    Enemy { key: "skeleton", name: "Skeleton", model: 1070, npc_param: 107102, think_param: 107001, area: "m31_00_00_00" },
    Enemy { key: "slave_knight_gael_1", name: "Slave Knight Gael 1", model: 6201, npc_param: 620100, think_param: 620100, area: "m51_01_00_00" },
    Enemy { key: "slave_knight_gael_2", name: "Slave Knight Gael 2", model: 6200, npc_param: 620000, think_param: 620000, area: "m51_01_00_00" },
    Enemy { key: "small_locust_preacher", name: "Small Locust Preacher", model: 6331, npc_param: 633101, think_param: 633111, area: "m51_00_00_00" },
    Enemy { key: "smouldering_ghru_grunt", name: "Smouldering Ghru Grunt", model: 1211, npc_param: 121120, think_param: 121020, area: "m38_00_00_00" },
    Enemy { key: "smouldering_writhing_flesh", name: "Smouldering Writhing Flesh", model: 2132, npc_param: 213200, think_param: 213200, area: "m38_00_00_00" },
    Enemy { key: "starved_hound", name: "Starved Hound", model: 2021, npc_param: 202100, think_param: 202101, area: "m30_00_00_00" },
    Enemy { key: "stray_demon", name: "Stray Demon", model: 5200, npc_param: 520000, think_param: 520000, area: "m33_00_00_00" },
    Enemy { key: "sulyvahns_beast", name: "Sulyvahn's Beast", model: 2250, npc_param: 225010, think_param: 225010, area: "m37_00_00_00" },
    Enemy { key: "tall_deacon_of_the_deep", name: "Tall Deacon of the Deep", model: 5222, npc_param: 522200, think_param: 522200, area: "m35_00_00_00" },
    Enemy { key: "tree_woman", name: "Tree Woman", model: 6060, npc_param: 606000, think_param: 606000, area: "m45_00_00_00" },
    Enemy { key: "twin_princes", name: "Twin Princes", model: 5250, npc_param: 525001, think_param: 525001, area: "m34_01_00_00" },
    Enemy { key: "twin_princes_helper", name: "Twin Princes Helper", model: 5251, npc_param: 525100, think_param: 525100, area: "m34_01_00_00" },
    Enemy { key: "vordt_of_the_boreal_valley", name: "Vordt of the Boreal Valley", model: 2240, npc_param: 224000, think_param: 224000, area: "m30_00_00_00" },
    Enemy { key: "wide_deacon_of_the_deep", name: "Wide Deacon of the Deep", model: 5221, npc_param: 522100, think_param: 522100, area: "m35_00_00_00" },
    Enemy { key: "winged_knight", name: "Winged Knight", model: 1290, npc_param: 129000, think_param: 129000, area: "m30_00_00_00" },
    Enemy { key: "wolf", name: "Wolf", model: 6040, npc_param: 604021, think_param: 604000, area: "m45_00_00_00" },
    Enemy { key: "wretch", name: "Wretch", model: 2070, npc_param: 207020, think_param: 207020, area: "m39_00_00_00" },
    Enemy { key: "yhorm_the_giant", name: "Yhorm the Giant", model: 5260, npc_param: 526000, think_param: 526000, area: "m39_00_00_00" },
];

/// An enemy by key ("black_knight") or name ("Black Knight"), ignoring case.
pub fn find(name: &str) -> Option<&'static Enemy> {
    let wanted = name.trim().to_lowercase();
    ENEMIES.iter().find(|e| e.key == wanted || e.name.to_lowercase() == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookups() {
        let k = find("Black Knight").unwrap();
        assert_eq!((k.model, k.npc_param, k.think_param, k.area), (1300, 130610, 130010, "m33_00_00_00"));
        assert!(find("black_knight").is_some());
        assert!(find("nobody").is_none());
    }

    #[test]
    fn keys_are_unique_and_snake_case() {
        let mut keys: Vec<&str> = ENEMIES.iter().map(|e| e.key).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), ENEMIES.len());
        assert!(ENEMIES.iter().all(|e| e.key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')));
    }

    /// The editor stubs' `kindle.enemy` alias lists exactly these keys.
    #[test]
    fn stubs_list_every_enemy() {
        let stubs = include_str!("../../../dist/stubs/kindle.lua");
        let alias = stubs.lines().find_map(|l| l.strip_prefix("---@alias kindle.enemy ")).expect("kindle.enemy alias");
        let in_stubs: Vec<&str> = alias.split('|').map(|k| k.trim().trim_matches('"')).collect();
        let keys: Vec<&str> = ENEMIES.iter().map(|e| e.key).collect();
        assert_eq!(in_stubs, keys);
    }
}
