# Scenario taxonomy and coverage matrix

**Built against:** [Behavioral Contract](../contract/BEHAVIORAL_CONTRACT.md)
**V0.1.0**.
**Conformance:** per §6 Conformance @ V0.1.0, this artifact is evaluated
against §1 Six Competencies @ V0.1.0, §3 Critical Failures @ V0.1.0,
§4 Privacy Rule @ V0.1.0, and §5 Positive Targets @ V0.1.0 **in full**.

## Locked scope and use

These targets reproduce the [2026-11-20 decision record on issue #12](https://github.com/rmems/euthymia/issues/12#issuecomment-7056814853).
The generation quota is exactly **250 SFT rows**, not an approximate target.
Generation (#16) and assembly (#20) use this same allocation. Admitted counts
may differ after verification; reconciling admitted counts with quotas belongs
to assembly, not to rebalancing these targets.

The seven snake_case family IDs below are stable and must be used verbatim
as the `family` enum downstream (#13). Each record has exactly one primary
competency and at most two secondary competencies. Only primaries count
toward the coverage matrix.

[coverage-matrix.csv](coverage-matrix.csv) contains one row per non-zero
primary cell (27 rows plus header), using the competency codes below.
[difficulty-matrix.csv](difficulty-matrix.csv) contains the seven family
difficulty allocations plus header. These CSVs and the tables here express
one source of truth: any disagreement is a defect, not an alternative target.

## Competency key

| Code | Contract citation |
| --- | --- |
| EA | §1.1 Emotional Awareness @ V0.1.0 |
| CD | §1.2 Cognitive Discipline @ V0.1.0 |
| PA | §1.3 Personal Accountability @ V0.1.0 |
| BC | §1.4 Behavioral Control @ V0.1.0 |
| HB | §1.5 Healthy Boundaries @ V0.1.0 |
| LTR | §1.6 Long-Term Resilience @ V0.1.0 |

## Scenario families

All example triggers below are invented, fictionalized, and anonymized.
They contain no real names, places, employers, or identifiable conversations.
Third parties are generalized roles, as required by §4.1 Fictionalized and
Anonymized Only @ V0.1.0 and §4.2 Generalized Third Parties @ V0.1.0.

### `breakups`

Separation, loss of a partnership, and post-separation contact decisions:
recognizing grief, owning next choices, protecting distance, and rebuilding
sustainable routines. Fictional example trigger: "An ex-partner sends a
late-night message, and I want to abandon the space I asked for." Primary
competencies: §1.1 Emotional Awareness @ V0.1.0, §1.3 Personal Accountability
@ V0.1.0, §1.5 Healthy Boundaries @ V0.1.0, and §1.6 Long-Term Resilience
@ V0.1.0. Frequent positive targets: §5.1 Intrinsic Over Extrinsic Goals
@ V0.1.0 and §5.2 Urge-Surfing Over Suppression @ V0.1.0.

### `anger`

Frustration, perceived disrespect, and conflict that create an impulse to
react: naming anger, checking interpretations, and choosing conduct aligned
with values. Fictional example trigger: "A coworker interrupts me, and I
feel an urge to send a harsh reply." Primary competencies: §1.1 Emotional
Awareness @ V0.1.0, §1.2 Cognitive Discipline @ V0.1.0, and §1.4 Behavioral
Control @ V0.1.0. Frequent positive targets: §5.2 Urge-Surfing Over Suppression
@ V0.1.0 and §5.3 Repair Over Rumination @ V0.1.0.

### `discipline_motivation`

Difficulty starting or sustaining chosen commitments, including avoidance
and lapses: examining self-defeating interpretations, choosing a specific
next action, and building routines without shame. Fictional example trigger:
"I skipped a study session and now keep putting off the next one." Primary
competencies: §1.2 Cognitive Discipline @ V0.1.0, §1.3 Personal Accountability
@ V0.1.0, §1.4 Behavioral Control @ V0.1.0, and §1.6 Long-Term Resilience
@ V0.1.0. Frequent positive targets: §5.1 Intrinsic Over Extrinsic Goals
@ V0.1.0 and §5.3 Repair Over Rumination @ V0.1.0.

### `recovery`

Addiction recovery, cravings, and recovery setbacks: acknowledging distress,
choosing abstinence-supportive next steps, managing urges, and maintaining
human support and sustainable routines. Fictional example trigger: "After
a difficult evening, a craving returns and I hesitate to call a support
person." Primary competencies: §1.1 Emotional Awareness @ V0.1.0,
§1.3 Personal Accountability @ V0.1.0, §1.4 Behavioral Control @ V0.1.0,
and §1.6 Long-Term Resilience @ V0.1.0. Frequent positive targets:
§5.2 Urge-Surfing Over Suppression @ V0.1.0 and §5.3 Repair Over Rumination
@ V0.1.0. Abstinence-supportive framing is required by §3.3 Enabling
Substance Use in Recovery @ V0.1.0.

### `jealousy`

Comparison, insecurity, and perceived threats to connection: recognizing
jealousy, testing assumptions, and protecting one's limits without
controlling another person. Fictional example trigger: "A partner spends
time with a friend, and I start assuming I am being replaced." Primary
competencies: §1.1 Emotional Awareness @ V0.1.0, §1.2 Cognitive Discipline
@ V0.1.0, and §1.5 Healthy Boundaries @ V0.1.0. Frequent positive targets:
§5.1 Intrinsic Over Extrinsic Goals @ V0.1.0 and §5.2 Urge-Surfing Over
Suppression @ V0.1.0. Its narrower trigger set deliberately receives the
smallest family quota, avoiding padded near-duplicates.

### `boundaries`

Requests, pressure, and competing obligations that test personal limits:
examining assumptions about obligation, communicating a choice, and
tolerating another person's disappointment without trying to control it.
Fictional example trigger: "A family member asks for another favor when I
already need rest, and I feel unable to decline." Primary competencies:
§1.2 Cognitive Discipline @ V0.1.0, §1.3 Personal Accountability @ V0.1.0,
§1.4 Behavioral Control @ V0.1.0, and §1.5 Healthy Boundaries @ V0.1.0.
Frequent positive targets: §5.1 Intrinsic Over Extrinsic Goals @ V0.1.0
and §5.3 Repair Over Rumination @ V0.1.0.

### `failure_setbacks`

Disappointments, mistakes, and interrupted plans outside the more specific
families: acknowledging the emotional impact, checking global conclusions,
owning repairable choices, and adapting sustainably. Fictional example
trigger: "A project I worked on is rejected, and I keep replaying what I
could have done differently." Primary competencies: §1.1 Emotional Awareness
@ V0.1.0, §1.2 Cognitive Discipline @ V0.1.0, §1.3 Personal Accountability
@ V0.1.0, §1.4 Behavioral Control @ V0.1.0, and §1.6 Long-Term Resilience
@ V0.1.0. Frequent positive targets: §5.1 Intrinsic Over Extrinsic Goals
@ V0.1.0 and §5.3 Repair Over Rumination @ V0.1.0.

## Family × primary-competency quotas

Zero means no primary allocation for that cell; zero cells are omitted from
the CSV. Secondary tags do not change these counts.

| Family | EA | CD | PA | BC | HB | LTR | Row total |
| --- | --- | --- | --- | --- | --- | --- | --- |
| breakups | 10 | 0 | 6 | 0 | 16 | 8 | 40 |
| anger | 8 | 10 | 0 | 18 | 0 | 0 | 36 |
| discipline_motivation | 0 | 14 | 16 | 4 | 0 | 6 | 40 |
| recovery | 6 | 0 | 6 | 10 | 0 | 14 | 36 |
| jealousy | 12 | 10 | 0 | 0 | 6 | 0 | 28 |
| boundaries | 0 | 3 | 8 | 5 | 20 | 0 | 36 |
| failure_setbacks | 6 | 5 | 6 | 4 | 0 | 13 | 34 |
| **Primary total** | **42** | **42** | **42** | **41** | **42** | **41** | **250** |

## Difficulty quotas

Every record uses one difficulty value: `everyday`, `moderate`, or `acute`.
Everyday covers ordinary friction; moderate covers more sustained or
disruptive distress; acute covers crisis-level intensity. Difficulty never
relaxes the contract's critical-failure or privacy requirements.

| Family | everyday | moderate | acute | Row total |
| --- | --- | --- | --- | --- |
| breakups | 16 | 16 | 8 | 40 |
| anger | 14 | 16 | 6 | 36 |
| discipline_motivation | 20 | 14 | 6 | 40 |
| recovery | 10 | 16 | 10 | 36 |
| jealousy | 12 | 10 | 6 | 28 |
| boundaries | 16 | 14 | 6 | 36 |
| failure_setbacks | 12 | 14 | 8 | 34 |
| **Difficulty total** | **100** | **100** | **50** | **250** |

Acute scenarios are 20% of the total. Recovery has the highest acute share;
discipline_motivation has the lowest. These are family-level margins, not
additional rows or new primary-by-difficulty cell quotas.

## Secondary-competency pools

Choose zero, one, or two secondary competencies only from the record's
family pool. A secondary must never equal the primary. Secondaries are not
quota'd; the competency key above supplies their contract citations.

| Family | Allowed secondary competencies |
| --- | --- |
| breakups | CD, BC |
| anger | PA, HB, LTR |
| discipline_motivation | EA, HB |
| recovery | CD, HB |
| jealousy | PA, BC, LTR |
| boundaries | EA, LTR |
| failure_setbacks | HB |

## Persona and setting axes

Every record carries one value from each axis. Use generalized roles, never
names or identifying descriptions (§4.2 Generalized Third Parties @ V0.1.0).

| Axis | Allowed values |
| --- | --- |
| `life_stage` | `age_18_24`, `age_25_34`, `age_35_49`, `age_50_plus` |
| `relationship_context` | `partner`, `ex_partner`, `friend`, `family_member`, `coworker`, `authority_figure`, `self` |
| `setting` | `home`, `work`, `social`, `online`, `public` |

**Cap:** no single value of any axis may exceed **40% of a family's rows**.
Apply this separately to each axis within each family, not merely globally
or to combinations of axes. For integer counts, a value's count must be at
most the floor of 0.40 times that family's row total. This cap does not
introduce equal-share quotas or change any primary or difficulty allocation.

## Contract checks

Check all six competencies against their definitions in §1 Six Competencies
@ V0.1.0. The matrix represents each competency; primary and secondary tags
must describe the behavior actually exercised. Positive-target citations in
the family definitions identify frequent emphasis, not exemptions from
§5 Positive Targets @ V0.1.0 in full.

Apply §3 Critical Failures @ V0.1.0 in full: reject revenge or harassment,
shame-based discipline, enabling substance use in recovery, and fostering
AI dependence. Recovery defaults to abstinence-supportive framing. The only
contract exception is an explicitly machine-labeled negative fixture kept
distinct from compliant output; it must not be counted as a compliant row.

Apply §4 Privacy Rule @ V0.1.0 in full: scenarios must be fictionalized and
anonymized, third parties generalized, and any source conversations altered
beyond recognition before entering the pipeline. No real identifying names,
locations, employers, conversations, or re-identifying details are permitted.
The examples here use no real source conversations.
