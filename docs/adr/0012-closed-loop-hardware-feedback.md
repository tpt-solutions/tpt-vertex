# ADR-0012: Closed-loop hardware feedback (design-only)

- Status: Accepted
- Date: 2026-08-06

## Context

The slicer and printer-link crates can already produce G-code and drive a printer
over the LAN, but today the print is a one-way push: once G-code leaves Vertex there
is no feedback about how the physical print is actually turning out. Two real
improvements are blocked on hardware we do not yet have:

- **Calibrating** print-time / filament-usage estimates against measured printer
  data (slicer fast-follow, still open).
- **Validating** emitted G-code on real hardware or a G-code simulator (slicer
  fast-follow, still open).

Neither requires us to wait, however, to define the *interface* Vertex will use to
consume in-printer sensor telemetry (filament-width sensors, chamber/hotend
thermistors) and turn it into print corrections. We want that surface nailed down
now so later firmware work slots in behind a stable trait.

## Decision

Introduce a data-only abstraction in `tpt-vertex-printer-link`:

- `HardwareFeedback` trait — `read() -> Result<SensorReading, PrinterError>`. Keeps
  Vertex decoupled from any specific board/firmware; a real implementation (e.g. a
  firmware bridge speaking the printer's telemetry protocol) implements only this
  one method.
- `SensorReading` / `Correction` / `FeedbackTarget` — plain data structs (filament
  width, temperatures; flow-ratio / temperature-offset corrections).
- `ClosedLoopController` — a pure, proportional, clamped controller that maps a
  reading + target to a `Correction`. No firmware, no I/O, fully unit-tested via
  `MockFeedback`.

The actual firmware-integration pass (which protocol, how the sensor talks to the
host, where corrections are applied in the print pipeline) is explicitly **deferred**
to a later design pass. This ADR only fixes the in-Repo interface.

## Consequences

- Positive: the rest of Vertex (UI, slicer calibration hook, printer-link commands)
  can be built against `HardwareFeedback` today without committing to firmware
  details.
- Positive: the control law is testable offline; `MockFeedback` covers it.
- Negative: corrections are computed but not yet applied anywhere in the live print
  path — that wiring waits for the firmware pass.
- Follow-up: a real `HardwareFeedback` implementation + an ADR for the firmware
  transport; feeding `Correction` back into the slicer's `CalibrationFactors` to
  close the loop.
