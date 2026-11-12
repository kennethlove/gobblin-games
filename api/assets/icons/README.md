# Icon slots

Every place an icon does, should, or could appear — one slot per line,
exactly one icon or none. `in use` = wired today (payments live in
`narrative_icon()` / template `| icon` calls); `candidate` = a shipped
sprite that fits, not yet wired; `none` = nothing in either sheet fits.
Licensing/provenance: [ATTRIBUTIONS.md](ATTRIBUTIONS.md).

| Slot | Icon | Status |
|---|---|---|
| UI: Back link (create game, timeline, my goblins) | arrow-left | in use |
| UI: Check-email page | mail | in use |
| UI: Timeline eliminations | skull | in use |
| UI: Timeline departed goblins | ghost | in use |
| UI: Replay page | swords | in use |
| UI: Log filter: All | list | in use |
| UI: Log filter: Deaths | skull | in use |
| UI: Log filter: Combat | sword | in use |
| UI: Log filter: Alliances | users | in use |
| UI: Log filter: Movement | map-pin | in use |
| UI: Log filter: Items | backpack | in use |
| Event: AddictionAcquired | pill | candidate |
| Event: AddictionCraving | pill | candidate |
| Event: AddictionEscalated | pill | candidate |
| Event: AddictionForgotten | pill | candidate |
| Event: AddictionHabituated | pill | candidate |
| Event: AddictionObserved | pill | candidate |
| Event: AddictionReinforced | pill | candidate |
| Event: AddictionRelapse | pill | candidate |
| Event: AddictionResisted | pill | candidate |
| Event: AfflictionAcquired | first-aid | candidate |
| Event: AfflictionCascaded | first-aid | candidate |
| Event: AfflictionHealed | health_potion | in use |
| Event: AfflictionProgressed | first-aid | candidate |
| Event: AllianceDissolved | hearts | candidate |
| Event: AllianceFormed | hearts | in use |
| Event: AllianceProposed | hearts | candidate |
| Event: AreaClosed | — | none |
| Event: Ate | trail_mix | in use |
| Event: BetrayalTriggered | switchblade | in use |
| Event: CharacterAttacked | pointy_sword | in use |
| Event: CharacterBledOut | recently_dead | in use |
| Event: CharacterDehydrated | dehydrated | in use |
| Event: CharacterDesperate | — | none |
| Event: CharacterDiedWhileTrapped | dead | candidate |
| Event: CharacterHidden | map-pin | candidate |
| Event: CharacterKilled | dead | in use |
| Event: CharacterMoved | map-pin | candidate |
| Event: CharacterRested | sleeping-bag | candidate |
| Event: CharacterSlept | sleeping-bag | candidate |
| Event: CharacterStarved | starving | in use |
| Event: CharacterTrapped | pit | candidate |
| Event: CharacterWoke | sleeping-bag | candidate |
| Event: CharacterWounded | pointy_sword | in use |
| Event: Combat | pointy_sword | in use |
| Event: CombatBeat | pointy_sword | in use |
| Event: CombatEnded | pointy_sword | in use |
| Event: CombatJoined | pointy_sword | in use |
| Event: CombatRoundStart | pointy_sword | in use |
| Event: ConditionAcquired | first-aid | candidate |
| Event: ConditionResolved | health_potion | in use |
| Event: CycleEnd | — | none |
| Event: CycleStart | — | none |
| Event: DaySummary | forecast | candidate |
| Event: Drank | water-bottle | candidate |
| Event: FixationAcquired | spell | candidate |
| Event: FixationConsummated | spell | candidate |
| Event: FixationEscalated | spell | candidate |
| Event: FixationFaded | spell | candidate |
| Event: FixationFired | spell | candidate |
| Event: FixationThwarted | spell | candidate |
| Event: Foraged | trail_mix | in use |
| Event: GameEnded | skull | candidate |
| Event: Generic | — | none |
| Event: HungerBandChanged | snack | candidate |
| Event: ItemDropped | backpack | candidate |
| Event: ItemFound | backpack | candidate |
| Event: ItemUsed | backpack | candidate |
| Event: PartialRescueProgress | first-aid | candidate |
| Event: PatronGift | — | none |
| Event: PhaseEnded | — | none |
| Event: PhaseStarted | — | none |
| Event: PhobiaAcquired | hallucinogen | candidate |
| Event: PhobiaEscalated | hallucinogen | candidate |
| Event: PhobiaForgotten | hallucinogen | candidate |
| Event: PhobiaHabituated | hallucinogen | candidate |
| Event: PhobiaObserved | hallucinogen | candidate |
| Event: PhobiaTriggered | hallucinogen | candidate |
| Event: RescueAttempted | first-aid | candidate |
| Event: SanityBreak | — | none |
| Event: ShelterSought | shelter | candidate |
| Event: SleepIncident | — | none |
| Event: StaminaBandChanged | — | none |
| Event: Struggling | — | none |
| Event: SubstanceUsed | pill | candidate |
| Event: ThirstBandChanged | water-bottle | candidate |
| Event: TrapSet | pointy_sword | in use |
| Event: TrapTriggered | pointy_sword | in use |
| Event: TrappedEscaped | — | none |
| Event: TraumaAcquired | spell | candidate |
| Event: TraumaAvoidance | spell | candidate |
| Event: TraumaEscalated | spell | candidate |
| Event: TraumaFlashback | spell | candidate |
| Event: TraumaForgotten | spell | candidate |
| Event: TraumaHabituated | spell | candidate |
| Event: TraumaObserved | spell | candidate |
| Event: TraumaReinforced | spell | candidate |
| Event: TrustShockBreak | hearts | candidate |
| Event: WoundAmputated | wounded | in use |
| Event: WoundBled | wounded | in use |
| Event: WoundHealed | health_potion | in use |
| Event: WoundInfected | infection | in use |
| Event: WoundInflicted | wounded | in use |
| Event: WoundTreated | health_potion | in use |
| Area event: Fire | burned | in use |
| Area event: Flood | river | in use |
| Area event: Earthquake / generic hazard | falling_rocks | in use |
| Area event: Storm | electrocuted | in use |
| Area event: Mutts | wolf | in use |
| Area event: Other | — | none |
| Item type: Weapon | sword | candidate |
| Item type: Shield | shield | candidate |
| Item type: Armor | — | none |
| Item type: Consumable | pill | candidate |
| Item type: Food | snack | candidate |
| Item type: Water | water-bottle | candidate |

170 narrative sprites still have no slot at all (inventory,
no caller):

`acacia` · `alcohol` · `amplitude` · `amputation` · `android` · `annoyed` · `antidepressant` · `award` · `awl` · `balance` · `barbed-wire` · `bear` · `blast` · `bomb` · `bong` · `boomerang` · `bottle` · `bow` · `broken-heart` · `broken_bone` · `buffalo` · `c4` · `cactus` · `calm` · `campfire` · `camping` · `canned-food` · `cave` · `cell-division` · `chocolate-bar` · `cigarettes` · `cloud` · `cloudy` · `coconut` · `conflict` · `confusion` · `coronavirus` · `crack` · `creativity` · `danger` · `dead-tree` · `deaf` · `deer` · `depression` · `desert` · `dna-structure` · `drone` · `drought` · `drowning` · `drug-test` · `elephant` · `examination` · `eye-drop` · `field` · `fire` · `fishing-hook` · `fishing_net` · `fist` · `fizzing_flask` · `flood` · `focus` · `forest` · `freeze` · `frozen_body` · `fruit-tree` · `gazelle` · `gorilla` · `grand-canyon` · `grass` · `grenade` · `gusts` · `hail` · `hallucination` · `hand-sanitizer` · `handshake` · `harpoon_trident` · `hazardous` · `haze` · `heat_haze` · `heroin` · `hexagon` · `hide` · `high_shot` · `hippopotamus` · `honeycomb` · `horse` · `hyena` · `hypodermic_test` · `illness` · `injection` · `insomnia` · `island` · `jaguar` · `landslide` · `large-intestine` · `lighter` · `lighting` · `lion` · `lunges` · `lungs` · `match-stick` · `mauled` · `meteor` · `microphone` · `mine` · `monkey` · `mountain` · `mouse` · `negative` · `no-water` · `obsession` · `opium` · `overdose` · `painkiller` · `panic` · `parachute` · `patch` · `patient` · `pelvis` · `phobia` · `plain_dagger` · `poison_bottle` · `polar-bear` · `pond` · `positive` · `powder` · `power` · `psychology` · `radiation` · `radioactive` · `rain` · `rainforest` · `redness` · `reed` · `reeds` · `rhinoceros` · `ribcage` · `ruin` · `sand` · `sandstorm` · `scale` · `self-harm` · `sleepy` · `slingshot` · `snowy` · `spear_hook` · `spider` · `spiked_mace` · `spinning_top` · `spray` · `stone` · `sun` · `swiss-army-knife` · `thunderstorm` · `tiger` · `tomb` · `torch` · `tornado` · `toxic` · `tree` · `tsunami` · `unity` · `vomiting` · `walker` · `wasp` · `water-flask` · `well` · `wood_axe` · `wooden-box` · `zombie`
