# AI as Lubricant for HEP Experiments

**Working title** — talk outline (living document)

---

## I. Opening

- Fake boot sequence / Terminator aesthetic hook
- Framing: this is not a tools demo — it's about how the cost structure of experimental HEP work is changing and what that means for collaborations

## II. The Overhead Problem

- What a working physicist actually spends their time on: legacy code, procurement, EVMS, documentation, format translation, institutional archaeology
- The ratio of physics to non-physics work is worse than anyone admits
- Concrete examples from HGCAL and DUNE work at FSU

## III. AI as Friction Remover

- LLMs are well-matched to the overhead layer: summarizing, drafting, navigating, translating between formats
- Examples: BCR narratives, PCB vendor communication, grading support, Altium debugging, boilerplate code
- Where they fail and quietly mislead — calibrating trust

## IV. The Marginal Cost Shift

- The bigger point: the set of things worth attempting has changed
- Cheaper exploration means more branches tried, bad ones killed faster
- Throwaway code as first-class citizen — checks you used to skip now get run
- Prototype-heavy hardware workflows: glue code, test stand scripts, parsers
- The cost of NOT exploring rises — static groups fall behind on a timescale nobody's calibrated for
- Negative rebound: more output can also mean more noise (transition to section V)

## V. Failure Modes: The Hanoi Rat Bounty

- The French colonial rat tail bounty — optimizing for producing evidence of work rather than doing work
  - French administration in Hanoi offered a bounty per rat tail to control sewer rats
  - Workers caught rats, cut off tails, released the rats to breed more
  - Some residents started rat farms
  - The bounty program made the rat problem worse by creating an economy optimized for producing evidence of rat-killing rather than actually killing rats
- AI makes it trivially cheap to produce "tails": plots, studies, code, status reports
- Nobody's checking whether the rats are actually dead
- "I asked Claude and it said…" as a new failure mode in meetings
- The bottleneck moves from implementation to taste — knowing which explorations matter

## VI. What's Actually at Risk: Mêtis and Tacit Knowledge

- **James C. Scott, *Seeing Like a State* (1998):** high-modernist schemes fail when they replace *mêtis* (practical local knowledge) with legible simplifications
  - Prussian scientific forestry: beautiful monocultures that collapsed a generation later because they eliminated the ecological complexity they didn't understand
  - The Hanoi sewer system itself as a legibility project that created the rat problem in the first place — the French built European-style infrastructure in a tropical colonial city, creating the rat habitat
- Large HEP collaborations are already high-modernist institutions — centralized frameworks, standardized procedures, top-down organization
- The *mêtis* is everything that actually makes them work: hallway knowledge, debugging sessions, shared vendor frustrations, undocumented tribal lore
- AI optimizes the legible layer and risks eroding the illegible one
- "The most dangerous moment for a complex system is when the people running it become convinced they finally understand it well enough to rationalize it"
- **Harry Collins, *Tacit and Explicit Knowledge* (2010):** interactional vs. contributory expertise, studied specifically in gravitational wave physics — directly relevant to how HEP collaborations transmit knowledge across generations of students and postdocs

## VII. Who Pays

- Overhead isn't just waste — it's scaffolding for FTEs, funding lines, training pipelines
- Automating the BCR saves the PI time and erodes the line item for the person who drafted it
- Students learned the experiment through the busywork — what replaces that apprenticeship?
- Technical staff whose value was partly measured in tasks now being absorbed
- The displacement question: reinvestment vs. attrition

## VIII. What Collaborations Should Actually Do

*(to be developed)*

- Invest in the *mêtis* layer deliberately — the informal knowledge transfer that AI can't see
- Reorganize reward structures before the rat farms get built
- Open questions for discussion

## IX. Close

- Kyle Reese GIF
- "Come with me if you want to live" or equivalent

---

## Key References

- **Scott, James C.** *Seeing Like a State: How Certain Schemes to Improve the Human Condition Have Failed.* Yale University Press, 1998.
- **Collins, Harry.** *Tacit and Explicit Knowledge.* University of Chicago Press, 2010.
- **Vann, Michael G.** "Of Rats, Rice, and Race: The Great Hanoi Rat Massacre, an Episode in French Colonial History." *French Colonial History* 4 (2003): 191–203.

## Presentation Format Notes

- Considering a TUI-based presentation using Ratatui (Rust) or presenterm (markdown-based terminal presenter)
- Green-on-black phosphor aesthetic / Terminator visual language
- Kyle Reese GIFs converted to ASCII/ANSI animation via chafa for terminal display
- Possible hybrid: presenterm for slide spine + embedded Ratatui demo segment
- Fake boot sequence opener: "CYBERDYNE SYSTEMS MODEL 101 / SCINTILLATOR CALORIMETRY DIVISION / INITIALIZING..."

## Working Title Alternatives

- AI as Lubricant for HEP Experiments *(current favorite)*
- AI as Friction Remover for HEP Experiments
- AI as Overhead Reducer for HEP Experiments

## Ideas Parking Lot

- The printing press analogy: scribes who became editors thrived; those who insisted on hand-copying didn't
- John Henry — competing with the machine on the machine's terms misses the point
- The actual Luddites: not anti-technology but skilled workers with a precise economic grievance about deskilling
- Sorcerer's Apprentice (well-worn but usable if sharpened): the brooms multiplying = proliferating plausible-looking analyses
- Goodhart's Law: when the measure becomes the target, it ceases to be a good measure
- Cobra Effect (less documented cousin of the Hanoi rat bounty)
- Cargo cult physics (Feynman): analysis-shaped output that lacks understanding
- Baumol's cost disease: physics judgment can't be automated but is bundled with tasks that can
- Jevons Paradox: well-established framing for the marginal cost argument (section IV) — available if needed for audiences who want the economics name
- Polanyi's Paradox: "we know more than we can tell" — Autor (2014) named it; available as shorthand for the Collins/Scott tacit knowledge argument
- Skill-biased technological change: returns rise for taste/direction, fall for implementation patience
- Transaction cost economics (Coase): AI lowers coordination costs, potentially making smaller groups viable
- Schumpeter's creative destruction
