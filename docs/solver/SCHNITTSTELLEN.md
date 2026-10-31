# Schnittstellen des PrePoLix-Solvers (`plxs`)

Stand M0 (2026-10-31). Dieses Dokument legt fest, worauf sich die Threads der Meilensteine
M1 bis M4 verlassen können: den Crate-Schnitt unter `solver/`, die IR in `plxs-core`, die
Trait-Signaturen der Fachcrates und die Regeln für Änderungen daran. Das Konzept dazu steht in
`KONZEPT_EIGENER_SOLVER.md` (Projektordner, Abschnitt 3).

## Grundregeln

- **Kein CalculiX-Code.** CalculiX steht unter GPL-2.0-only, PrePoLix unter GPL-3.0-or-later;
  die beiden sind nicht kompatibel. Es wird keine Zeile aus den Fortran- oder C-Quellen
  übernommen, übersetzt oder abgeschrieben, auch keine Tabellen. Erlaubt sind das
  CalculiX-Handbuch (Verfahrensbeschreibung), Lehrbücher und ccx als Black-Box-Orakel für
  Referenzergebnisse. Jede Crate nennt ihre Quellen im `//!`-Kommentar.
- **Ein Numerik-Ökosystem.** Lineare Algebra ausschließlich über `faer` (dicht und sparse,
  Ordering, dichte Eigenlöser), Parallelität über `rayon`, `num-complex` nur als Zahltyp.
  Kein nalgebra, ndarray, sprs, russell oder Ähnliches daneben. Eigenlöser (Lanczos/Arnoldi)
  werden auf `faer` aufgesetzt, nicht aus einer weiteren Crate bezogen. Die Versionen stehen in
  `[workspace.dependencies]`.
- **Workspace-Lints gelten**: `unsafe_code = "deny"`, `clippy::all`; `cargo fmt`.
- **Alles cross-platform** (Linux und Windows), reines Rust, keine C/Fortran-Abhängigkeiten.
- **Jeder PR bringt Fixtures mit** (siehe `FIXTURES.md`): Die Fixtures seiner Stufe müssen
  mit `plxs-fixtures run` grün sein, bevor er gemerged wird.

## Crate-Schnitt

Alle Crates liegen unter `solver/plxs-*` (Workspace-Member `solver/plxs-*`), Präfix `plxs-`.
Abhängigkeiten zeigen nur nach unten:

```
plxs-cli            Binary `plxs`: ccx-kompatible Kommandozeile, stdout-Protokoll, Exit-Codes
  └ plxs-session    Session-API: Modell laden, Steps rechnen, Zustand, Ergebnisse, Restart
      ├ plxs-input      .inp -> IR (M0, fertig)
      ├ plxs-output     .frd (ASCII/binär), .dat, .sta, .cvg; Expansion der Ergebnisse (M1)
      └ plxs-procedures Static, Nlgeom/Newton, Visco, Dynamic, Heat, Coupled, Frequency,
            │           Buckle, Complex Frequency, Modal/Steady State Dynamics
            ├ plxs-assembly   DOF-Nummerierung, MPC-Elimination, CSR-Muster, parallele Assemblierung
            │   ├ plxs-elements   Formfunktionen, Integration, K/M/f_int je Elementtyp, Expansion
            │   ├ plxs-materials  Elastisch, Plastisch, Kriechen, thermisch, Zustandsvariablen
            │   ├ plxs-constraints SPC, Equation, Rigid Body, Tie, Transform, Cyclic, Submodel
            │   └ plxs-contact    Suche, N2S/S2S/Mortar, Surface Behavior, Reibung, Gap Conductance
            └ plxs-linalg     Sparse-Strukturen, LDLᵀ/LU (faer), CG, Eigenlöser
plxs-core           IR: Modell, Steps, State, Results, Progress, Fehler. Keine Abhängigkeit nach oben.
plxs-verify         Fixture-Korpus, Golden Files, Vergleich, Binary `plxs-fixtures`
```

`plxs-core`, `plxs-input`, `plxs-verify` und `plxs-cli` sind in M0 gefüllt; die übrigen Crates
existieren leer mit einem `//!`-Kommentar und werden von den Threads der Stufen gefüllt.
`plxs-core` hängt bewusst nicht von `plx-model`/`plx-mesh` ab, damit der Solver ohne PrePoLix
gebaut und getestet werden kann; `plxs-verify` nutzt `plx-io` nur zum Lesen von `.frd`/`.dat`.

## Die IR (`plxs-core`) ist der Vertrag

`plxs_core::Analysis { model: Model, steps: Vec<Step> }` ist das vollständige Datenmodell
(Dokumentation in den Modulen `mesh`, `element`, `material`, `section`, `constraints`,
`contact`, `amplitude`, `step`, `model`, `state`, `results`, `progress`, `error`). Regeln:

1. **Nutzer-Ids bleiben** (Knoten- und Elementnummern der `.inp`); intern wird über dichte
   Indizes gerechnet (`Nodes::index`, `Model::element_index`), die Ausgabe schreibt Ids.
2. **Namen in Großbuchstaben** (Sets, Surfaces, Materialien, Interaktionen, Amplituden), wie
   CalculiX sie liest (`normalized_name`).
3. **Steps sind vollständig.** `Step` enthält, was während des Steps gilt; die
   Carry-over-Regeln von CalculiX (`OP=NEW`/`OP=MOD` für Randbedingungen, Lasten,
   Temperaturen; Ausgabeanforderungen werden übernommen, Lasten eines Perturbation-Steps
   nicht) löst `plxs-input` auf. Prozeduren schauen nie in frühere Steps.
4. **Alles ist `Clone + Serialize + Deserialize`** (serde), auch `State` und `Results`, damit
   Phase 2 Zustände zwischen Nodes reichen und Restarts speichern kann. Nach dem
   Deserialisieren `Model::rebuild_indices()` aufrufen.
5. **Keine Rechenlogik in `plxs-core`**: nur Daten, `validate` (Referenzen, Sections
   zuweisen) und Fehlertypen.
6. **Fehler tragen den Item-Namen** (`Error::Input { keyword, message, location }`,
   `UndefinedReference { keyword, kind, name }`) und formatieren sich im ccx-Wortlaut
   (`*ERROR reading *BOUNDARY: ...`, `*ERROR in calinput: keyword *X cannot be interpreted`),
   weil `solver_check` und der Job-Monitor den Text lesen.
7. **Änderungen an der IR sind ein eigener, kleiner PR** mit Begründung, den der Koordinator
   reviewt; Threads hängen Felder an (`Option<..>` mit `Default`), statt Bestehendes
   umzubauen. Ein PR, der nebenbei die IR ändert, wird nicht angenommen.

Was die IR heute schon abdeckt, das aber erst später gerechnet wird: alle Elementtypen des
Inventars (`ElementType`), alle Prozeduren (`Procedure`), Kontakt, zyklische Symmetrie,
Submodelle, `*SENSITIVITY` (nur geparst; rechnet weiter mit ccx), `*RESTART`, `*CONTROLS`.

## Session-API (`plxs-session`, M1 T3)

```rust
pub struct SessionOptions { pub threads: usize, pub work_dir: PathBuf }

impl Session {
    pub fn new(model: Model, options: SessionOptions) -> Result<Session>;
    /// Rechnet einen Step auf dem aktuellen Zustand; Ereignisse gehen an `progress`.
    pub fn run_step(&mut self, step: &Step, progress: &mut dyn Progress) -> Result<StepOutcome>;
    pub fn state(&self) -> &State;
    pub fn results(&self) -> &Results;
    /// Zustand plus Modell-Fingerprint für `*RESTART, WRITE` (postcard-serialisiert).
    pub fn snapshot(&self) -> Snapshot;
    pub fn resume(model: Model, snapshot: Snapshot, options: SessionOptions) -> Result<Session>;
}

pub enum StepOutcome { Converged, Diverged { increment: u32, message: String }, Partial { increments: u32 } }
```

Ein `rayon`-Pool je Session mit `threads` Threads (Default: `OMP_NUM_THREADS`, sonst alle
Kerne). Kein globaler Zustand: mehrere Sessions pro Prozess müssen möglich sein.

## Traits der Fachcrates

Die Signaturen sind die Vorgabe für M1/M2; Abweichungen im Detail (Lebensdauern, Puffer)
entscheidet der Thread, der die Crate füllt, und dokumentiert sie hier im selben PR.

### `plxs-linalg` (M1 T1)

```rust
/// Symmetrisches CSR-Muster (obere Hälfte) mit Werten; Indizes u32, Werte f64.
pub struct SparseMatrix { .. }
pub struct Pattern { .. }           // einmal je Step berechnet, über Iterationen wiederverwendet

pub trait LinearSolver {
    /// Symbolische Analyse (Ordering, Supernodes) für ein Muster; wird wiederverwendet.
    fn analyze(&mut self, pattern: &Pattern) -> Result<()>;
    /// Numerische Faktorisierung der Werte zum zuletzt analysierten Muster.
    fn factorize(&mut self, matrix: &SparseMatrix) -> Result<()>;
    /// Löst K x = b für eine oder mehrere rechte Seiten (in place).
    fn solve(&self, rhs: &mut [f64]) -> Result<()>;
    /// Trägheit (Anzahl negativer Pivots) nach LDLᵀ, für den Sturm-Check der Eigenlöser.
    fn inertia(&self) -> Option<usize>;
}
// Implementierungen: LdltSolver (faer, symmetrisch), LuSolver (faer, unsymmetrisch:
// Reibung, Coriolis), CgSolver (Jacobi-/ILU-vorkonditioniert) für die GUI-Optionen
// "Iterative Scaling/Cholesky".

pub trait EigenSolver {
    /// Die `count` kleinsten Eigenpaare von K x = λ M x (Shift-Invert-Lanczos auf LDLᵀ),
    /// mit Sturm-Sequenz-Prüfung, dass keiner verpasst wurde.
    fn solve(&mut self, k: &SparseMatrix, m: &SparseMatrix, count: usize, shift: f64) -> Result<EigenPairs>;
}
```

### `plxs-elements` (M1 T2, M2 T4/T5)

```rust
pub trait ElementFormulation {
    fn kind(&self) -> ElementType;
    /// Freiheitsgrade je Knoten (3, 6, 2 oder 1 für Wärme), in der Reihenfolge der IR.
    fn dofs_per_node(&self) -> usize;
    fn integration(&self) -> &IntegrationRule;
    /// Tangentensteifigkeit und innere Kräfte am aktuellen Zustand (linear: K, f_int = K u).
    fn stiffness(&self, ctx: &ElementContext, out: &mut ElementMatrix) -> Result<()>;
    fn internal_force(&self, ctx: &ElementContext, out: &mut [f64]) -> Result<()>;
    fn mass(&self, ctx: &ElementContext, lumped: bool, out: &mut ElementMatrix) -> Result<()>;
    /// Spannungen/Dehnungen an den Integrationspunkten in den Zustand schreiben.
    fn update_state(&self, ctx: &ElementContext, state: &mut ElementState) -> Result<()>;
    /// Extrapolation von Integrationspunkt-Werten an die Knoten (Matrizen wie im Handbuch).
    fn extrapolate(&self, values: &[f64], out: &mut [f64]);
    /// Flächenlast auf Face n -> äquivalente Knotenkräfte.
    fn face_load(&self, ctx: &ElementContext, face: u8, pressure: f64, out: &mut [f64]) -> Result<()>;
    /// Expansion in Volumenelemente für die `.frd` (Schalen, Balken, 2D), sonst `None`.
    fn expansion(&self, ctx: &ElementContext) -> Option<Expansion>;
}
// `shape`-Modul: Formfunktionen und Integrationsregeln aller Familien an einem Ort; auch
// Flächenlasten, Kontaktsuche und Extrapolation greifen darauf zu.
```

### `plxs-materials` (M2 T8/T9)

```rust
pub trait MaterialLaw {
    /// Zahl der Zustandsvariablen je Integrationspunkt.
    fn state_size(&self) -> usize;
    /// Spannung und konsistente Tangente für eine Dehnung (bzw. ein Dehnungsinkrement),
    /// Temperatur und Zeitschritt; aktualisiert die Zustandsvariablen.
    fn stress(&self, input: &MaterialInput, state: &mut [f64]) -> Result<MaterialOutput>;
    /// Wärmeleitung und spezifische Wärme bei Temperatur T (für thermische Elemente).
    fn thermal(&self, temperature: f64) -> ThermalProperties;
}
// Implementierungen: LinearElastic, J2Plastic (isotrop/kinematisch/kombiniert, Radial Return),
// NortonCreep (implizit), jeweils mit Temperaturinterpolation aus den IR-Tabellen.
```

### `plxs-constraints` (M2 T6)

```rust
/// Alle Zwangsbedingungen eines Steps als Transformationsmatrix T (u = T u_red + u_0):
/// SPCs, Equations, Rigid Bodies, Ties, Transforms, Cyclic, Submodel-Werte.
pub struct ConstraintSet { .. }
impl ConstraintSet {
    /// Baut die Eliminationsmatrix; löst Überschneidungen (SPC auf abhängigem DOF, zwei MPCs
    /// auf demselben DOF) durch Re-Pivotierung auf, statt abzubrechen.
    pub fn build(model: &Model, step: &Step, state: &State, dofs: &DofMap) -> Result<ConstraintSet>;
    pub fn reduce(&self, k: &SparseMatrix) -> SparseMatrix;        // Tᵀ K T
    pub fn reduce_rhs(&self, f: &[f64], out: &mut [f64]);
    pub fn expand(&self, u_red: &[f64], out: &mut [f64]);
    /// Nichtlineare MPCs (Rigid Body mit großen Rotationen) je Newton-Iteration neu.
    pub fn update(&mut self, state: &State) -> Result<()>;
}
```

### `plxs-contact` (M3 T11/T12)

```rust
pub trait ContactFormulation {
    /// Kontaktsuche: aktive Slave-Knoten/-Integrationspunkte und ihre Master-Faces.
    fn search(&mut self, geometry: &ContactGeometry, state: &State) -> Result<ContactSet>;
    /// Superset des Matrixmusters für diesen Step (Kontaktsuche-Ergebnis + Reserve).
    fn pattern(&self, set: &ContactSet) -> PatternContribution;
    /// Kontaktsteifigkeit und -kräfte für die aktiven Paare (Penalty: Federn; Mortar: Lagrange).
    fn assemble(&self, set: &ContactSet, state: &State, k: &mut SparseAssembler, f: &mut [f64]) -> Result<()>;
    /// Kontaktstatus, Druck, Spalt je Slave-Punkt für CDIS/CSTR/CELS und `*CONTACT PRINT`.
    fn outputs(&self, set: &ContactSet, state: &State) -> ContactOutputs;
}
// Implementierungen in dieser Reihenfolge: NodeToSurfacePenalty, SurfaceToSurfacePenalty, Mortar.
```

### `plxs-procedures` (M1 T3, M2 T7/T8/T9, M3)

```rust
pub trait Procedure {
    /// Rechnet den Step auf dem Zustand; schreibt Inkremente in `results`, meldet Ereignisse.
    fn run(&self, ctx: &mut ProcedureContext, step: &Step, progress: &mut dyn Progress) -> Result<StepOutcome>;
}
// ProcedureContext: Modell, DofMap, Assembler, Solver, State, Results, Thread-Pool.
// Static (linear), Nonlinear (Newton mit automatischer Inkrementierung nach *CONTROLS-Defaults),
// Frequency, Buckle, Dynamic (HHT-α / explizit), HeatTransfer, Coupled/Uncoupled,
// ModalDynamics, SteadyStateDynamics, ComplexFrequency, Visco.
```

### `plxs-output` (M1 T3)

```rust
pub fn write_frd(path: &Path, model: &Model, results: &Results, binary: bool) -> Result<()>;
pub fn write_dat(path: &Path, results: &Results) -> Result<()>;
pub fn write_sta(path: &Path, rows: &[StaRow]) -> Result<()>;
pub fn write_cvg(path: &Path, rows: &[CvgRow]) -> Result<()>;
```

Abnahme: `plx_io::frd::read_frd` liest die eigene und die ccx-Datei, die geparsten Strukturen
sind gleich (Klasse D); `plx_io::dat::parse_dat` entsprechend. Blocküberschriften der `.dat`
wörtlich wie in `crates/plx-io/src/dat.rs` dokumentiert.

## Progress und stdout-Protokoll

`plxs_core::Progress` empfängt `ProgressEvent` (Step, Increment, Iteration, IncrementDone,
Cutback, Eigenvalue, StepDone, Warning, Info, JobDone). `plxs-cli` druckt daraus CalculiX'
Zeilen, die `plx-job` (`live.rs`) und `solver_check` lesen:

```
This is Version <x>            (plxs -v; die Startzeile enthält das Wort CalculiX)
 STEP n
 increment n attempt m
 increment size= <f>
 actual step time= <f>
 iteration n
 Job finished
*ERROR ...                     (Abbruch, Exit-Code 1)
*ERROR in calinput: keyword *X cannot be interpreted
```

stdout wird nach jeder Zeile geflusht. Exit-Code 0 nur nach `Job finished`.

## Was M1 zu liefern hat (Abnahme)

- `solver_check::run` aus `plx-app` besteht mit `plxs` als Executable (Kragbalken mit Kraft
  und Druck, Selbsttest-Keywords).
- Alle Fixtures `static/*` (Stufe M1 in `FIXTURES.md`) sind grün:
  `cargo run -p plxs-verify --bin plxs-fixtures -- run --solver target/debug/plxs --area static`.
- Benchmark 176k-DOF-Block (Build-Doku) gegen ccx-PaStiX, 4 Threads, als Zahl im PR.
