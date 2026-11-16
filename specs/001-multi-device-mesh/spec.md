# Feature Specification: Multi-device mesh — one pointer, one keyboard, one clipboard across several devices

**Feature Branch**: `001-multi-device-mesh`

**Created**: 2026-11-16

**Status**: Clarified

**Input**: User description: "Add multi-device mesh to the desktop app, scoped first to cursor/keyboard edges and a pooled clipboard across up to ten devices, ported from the reference implementation rather than reinvented."

## Clarifications

### Session 2026-11-16

- **Q: How should multi-device coexist with the current two-device app?** → Always running, hidden until two devices are connected. There is no on/off switch and no restart: the capability is part of the app from start-up and stays dormant (no extra wire fields, no extra navigation, no changed behaviour) while only one device is connected. It activates the moment a second device connects.
- **Q: What may a phone do in the first slice?** → Leaf device: it can be driven and can take control back, but it is never asked to relay a crossing to a third device.
- **Q: Does the first slice include "pair once, meet everyone"?** → Yes, it is in scope for the first slice.
- **Q: Which platforms must be proven for slice 1 to be accepted?** → All four: macOS, Windows, Linux and Android (the phone as a leaf).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Several devices connected at the same time (Priority: P1)

I own a desktop, a laptop and a phone on the same network. Today I can only use one of them at a time: connecting the second pushes the first away. I want all of them connected at once — listed in one place, each one usable — without re-pairing or disconnecting.

**Why this priority**: Everything else in this feature depends on it. Without several simultaneous connections there is nothing to move a pointer or a clipboard between, and the current single-peer rule is the first thing a person hits.

**Independent Test**: Pair three devices, connect them all, and confirm they stay connected and appear together; then switch Multi-device off and confirm the app behaves exactly as the current two-device app does.

**Acceptance Scenarios**:

1. **Given** three devices are paired, **When** each connects, **Then** all three show as connected at the same time and none is disconnected by a later one.
2. **Given** three devices are connected, **When** I close the lid of one, **Then** the other two stay connected and the closed one is shown as gone.
3. **Given** ten devices are connected, **When** an eleventh tries to join, **Then** it is refused with a reason the person can understand and the ten keep working.
4. **Given** only one device is connected, **When** I use the app, **Then** it behaves exactly as it does today and nothing multi-device is visible.
5. **Given** one device is connected and I connect a second, **Then** multi-device activates and the Multi-device page appears without restarting the app, and the first connection is not disturbed.

---

### User Story 2 - The pointer and keyboard go where the screens are (Priority: P1)

My screens sit in a row: desktop, laptop, then the phone. I want to push the pointer through the edge of one screen and have it appear on the next, type there with the same keyboard, and bring it home by coming back through the edge — landing where I left, not in the middle.

**Why this priority**: This is the defining behaviour of the product. It is also the part with the most subtle failure modes (accidental handovers, control that cannot be taken back, a pointer that returns to the wrong place), so it must be right before more devices are added.

**Independent Test**: With three devices arranged side by side, drive each one from the first, and return home; repeat with the middle device powered off to confirm a crossing goes to the device beyond it or is refused cleanly.

**Acceptance Scenarios**:

1. **Given** the right edge of my screen is assigned to the laptop, **When** I rest the pointer on that edge for the configured hold, **Then** the laptop takes the pointer and the pointer appears on the laptop's facing edge at the same height.
2. **Given** I am driving the laptop and my hand returns past the point where it entered, **When** control comes home, **Then** the pointer lands on the seam spot I left by, carried by my travel, and does not immediately cross again.
3. **Given** the laptop's own far edge is assigned to the phone, **When** I keep pushing through the laptop, **Then** the laptop hands the pointer on to the phone and stops being driven itself, and coming back reverses exactly that chain.
4. **Given** another device already has the pointer, **When** two devices try to take it at the same moment, **Then** one keeps it, the other is told it was refused, and neither is left without a working pointer.
5. **Given** a device is assigned to an edge but is not connected, **When** I push into that edge, **Then** nothing is taken and the pointer stays home, with the reason visible.
6. **Given** I am typing while driving another device, **When** I use shortcuts, modifiers and characters from another keyboard layout, **Then** they arrive as the keys that device expects, and the local machine's keys are untouched.

---

### User Story 3 - One clipboard across every device (Priority: P1)

I copy something on the laptop and paste it on the desktop, or copy on the phone and paste into a window on either computer, without thinking about which device I am on.

**Why this priority**: It is the second half of the requested scope and the feature people use constantly. It also cannot be bolted on later without reworking how items are addressed once more than two devices are connected.

**Independent Test**: With three devices connected, copy in sequence on each and paste on the other two; interrupt the network during a copy and confirm the result is still the newest item and nothing loops.

**Acceptance Scenarios**:

1. **Given** three devices are connected, **When** I copy text on any one of them, **Then** it can be pasted on the other two within a second.
2. **Given** I copy twice in quick succession on different devices, **When** both items arrive, **Then** the one copied later is the one every device pastes.
3. **Given** an item arrives on a device that already has it, **When** that device's clipboard changes, **Then** the item is not sent back out, and no device receives it twice.
4. **Given** a device's clipboard is turned off or its receive direction is off, **When** items are published, **Then** that device neither receives nor contributes items, and no error is shown to the others.
5. **Given** an item larger than the configured limit, or marked concealed by the source system, **When** it is copied, **Then** it is not published.
6. **Given** an image is copied on one device, **When** images are allowed, **Then** it can be pasted on another; when images are not allowed, it is not sent.

---

### User Story 4 - Pair once, meet everyone (Priority: P2)

I pair my phone with my desktop. Without pairing it again with the laptop, it should appear to the laptop too, because they are all on the same desk and the same network.

**Why this priority**: In scope for the first slice. It is not needed to prove the pointer and clipboard work, but without it using several devices means pairing each pair by hand, which makes the feature impractical to set up and to test.

**Independent Test**: Pair a phone with one computer only, connect a second computer already known to the first, and confirm the phone appears on both without further pairing.

**Acceptance Scenarios**:

1. **Given** a phone is paired with one computer, **When** a second computer that already knows the first connects, **Then** the phone appears on the second computer without a new pairing code.
2. **Given** a device was forgotten on one computer, **When** it reconnects, **Then** it does not silently come back through another computer.

---

### User Story 5 - Crossings follow the monitors that actually touch (Priority: P3)

Two screens are lined up, but the taller one's edge is offset. I want the pointer to cross only where the two monitors overlap, keeping its place along the edge.

**Why this priority**: It removes the most common "the pointer went to the wrong place" complaint on mismatched screens, but the feature is useful without it: whole-edge arrangement works as it does today.

**Independent Test**: Arrange two monitors with a vertical offset and confirm crossing is refused where they do not touch and lands at the matching height where they do.

**Acceptance Scenarios**:

1. **Given** the laptop's monitor only overlaps the lower half of the desktop's monitor, **When** I push into the upper half of that edge, **Then** the pointer stays home.
2. **Given** they overlap, **When** I cross at the top of the overlap, **Then** the pointer arrives at the top of the laptop's facing edge.

---

### Edge Cases

- A device disappears mid-crossing: the handover must complete or be abandoned cleanly, and the pointer must never be left with no owner.
- A device is driven and its own pointer is moved by hand: it must take control back immediately, and the driver must be told.
- The person pushes into an edge assigned to a device that is connected but has cursor sharing switched off: the push must be refused with a visible reason and not retried in a loop.
- Ten devices are connected and one is a phone: the phone may end a chain but must not be asked to relay to another device.
- A copy arrives while the receiving device is applying its own copy: the newer item wins and neither loops.
- A device restarts: its published items must not be re-sent as new, and its sequence numbers must not repeat ones the others still remember.
- Two devices are assigned to the same side: the arrangement must keep one assignment and say which.
- A mesh device talks to an older app that knows nothing about several devices: it must behave as an ordinary single peer, with no extra fields and no errors.
- The pointer is parked in the middle of a screen while driving: coming home must never use that parked position as the place it left.

## Requirements *(mandatory)*

### Functional Requirements

**Connections and devices**

- **FR-001**: The system MUST keep up to ten devices connected at the same time, each with its own link, and MUST NOT disconnect an existing device when another connects.
- **FR-002**: The system MUST show every connected device with a name, a kind (computer or phone) and a reachable address, and MUST update that list as devices come and go.
- **FR-003**: The multi-device capability MUST be part of the app from start-up, with no switch and no restart, and MUST stay dormant while only one device is connected: no additional fields on the wire to that device, no additional navigation, and behaviour indistinguishable from the current app. It MUST activate as soon as a second device connects, without disturbing the existing connection.
- **FR-004**: Activating multi-device MUST keep the person's existing settings, pairings and preferences, and MUST NOT lose or silently change them; going back to a single device MUST leave the app exactly as it was before a second one connected.
- **FR-005**: When the device limit is reached, the system MUST refuse an additional device with a reason the person can act on.

**Pointer and keyboard**

- **FR-006**: Each side of a device's screen MUST be assignable to one other device, or to none; an assignment MUST survive a restart.
- **FR-007**: A crossing MUST happen only after the pointer has rested on an assigned edge for the configured hold, and MUST use the same hold and resistance rules on every supported platform.
- **FR-008**: On crossing, the receiving device MUST place its pointer at the matching position along the facing edge, and the driving device MUST keep its pointer out of the way without treating that as a new crossing.
- **FR-009**: Keyboard input MUST follow the pointer: while a device drives another, keys and modifiers go to the driven device only, with the key mapping that device expects.
- **FR-010**: Control MUST return when the hand travels back past the entry point, landing on the seam position the handover used, carried by the travel since, and MUST require a fresh hold before crossing again.
- **FR-011**: A driven device MUST be able to take control back when its own pointer or buttons are used, and the device that was driving it MUST be told and MUST stop sending input.
- **FR-012**: A driven device whose further edge is assigned to a third device MUST be able to hand the pointer on to that device, and the chain MUST unwind in reverse when the hand comes back.
- **FR-013**: A take MUST be accepted, refused or timed out explicitly, so that no device is ever left believing it has control that it does not have.
- **FR-014**: A device assigned to an edge but not connected, or with cursor sharing off, MUST produce a refusal the person can see, and MUST NOT be retried endlessly.
- **FR-015**: A phone MUST be usable as an end device — a computer can drive it and it can take control back — but it MUST NOT be asked to relay a crossing to a further device, and a crossing must never depend on a phone being in the chain.

**Clipboard pool**

- **FR-016**: A copy on any connected device MUST become pasteable on every connected device that accepts clipboard items.
- **FR-017**: The system MUST keep the most recently copied item, using a single shared notion of "later" that is stable across devices, and MUST NOT replace a newer item with an older one that arrives late.
- **FR-018**: The system MUST NOT send back an item it received, and each device MUST apply an item at most once, however many paths it reaches it by.
- **FR-019**: Existing clipboard controls (on/off, send direction, receive direction, images, size limit, concealed-content filtering) MUST keep working and apply per device.
- **FR-020**: A device that restarts MUST number its published items past the ones other devices still remember, so a repeated number cannot be mistaken for a duplicate.

**Compatibility and data**

- **FR-021**: Every field added to the network conversation MUST be optional and ignored by apps that do not know it, so a device running an older version keeps working as a single peer.
- **FR-022**: Assignments, the device list and the feature's own settings MUST be stored on the device that owns them and MUST NOT overwrite the existing settings for the two-device app.
- **FR-023**: The system MUST report, in a form the person can read, why a take or a copy did not happen (refused, not allowed, not connected, too large, concealed).
- **FR-024**: The pointer, keyboard and clipboard behaviour MUST be the same on macOS, Windows, Linux and Android, with the same hold, resistance and return rules; a platform MUST NOT cross instantly or ignore the configured hold.

### Key Entities

- **Device**: a computer or phone the person has paired; has a name, a kind, a stable identifier and a reachable address.
- **Link**: one direct connection between two devices; carries the conversation and its own liveness state. Each device keeps one per connected peer, with no relay in between.
- **Session**: what one device knows about one other device — whether it drives, is driven, or is idle, and which route number its current handover belongs to.
- **Edge assignment**: which device is behind each of the four sides of a screen, plus where along the screen's arrangement that device sits.
- **Clipboard item**: the copied content, when it was copied, on which device, and the identifiers needed to recognise it again.
- **Roster**: which device is connected to which other device, as each device sees it, so crossings can be routed through the mesh.
- **Handover record**: the side, the position along it, and the travel since, used to bring the pointer home.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With a single device connected, every existing two-device behaviour and its automated checks pass unchanged (no regression, no new required field on the wire, no new navigation).
- **SC-002**: Three devices stay connected for an hour of normal use with no disconnection caused by another device joining, and reconnection after a sleep happens without the person doing anything.
- **SC-003**: A pointer crossing between two connected devices completes within 250 ms of the hold expiring, on every supported platform.
- **SC-004**: A chain of three devices can be crossed forward and back ten times in a row without a lost pointer, a stuck keyboard, or a second unintended handover.
- **SC-005**: Coming home lands within 2 pixels of the recorded seam position plus travel, in 95 of 100 attempts, for crossings at any height of the edge.
- **SC-006**: With three devices connected, 100 copies made alternately across them result in every device holding the same, newest item, with no item applied twice and no copy re-sent by a receiver.
- **SC-007**: A clipboard item published anywhere is pasteable elsewhere within 1 second on the same network, up to the configured size limit.
- **SC-008**: An older-version device connected to a device with multi-device active but only one peer shows no errors and keeps its single-peer behaviour, verified by its own automated checks and by a manual session.
- **SC-009**: A person can set up three devices (pair the second to the first, assign each edge) in under five minutes without reading documentation.
- **SC-010**: Every acceptance scenario in User Stories 1–4 is demonstrated on macOS, Windows, Linux and Android, with the phone acting as a leaf device (driven and taking control back, never relaying).

## Assumptions

- Multi-device is always present and activates on the second connection; there is no on/off switch and no restart. While one device is connected the app must be indistinguishable from today's.
- Devices are on the same local network and reach each other directly; there is no relay server and no cloud component in this slice.
- Pairing, identity, encryption and the existing transport are reused as they are; no new trust model is introduced.
- Phones participate as leaf devices (they can be driven and can take control back) but do not relay crossings in this slice, and a phone still talks to one computer at a time.
- Up to ten devices, as in the reference implementation.
- Notifications pooling, files to several devices at once, targeting screen share/terminal/apps at a chosen device, and the monitor-level map (User Story 5) are separate slices; User Story 5 is specified here only because crossings must not be designed in a way that blocks it.
- The clipboard continues to handle text and images, with the existing size limit and concealed-content rules.
- The reference implementation is the behavioural authority where this specification is silent; where the two disagree on user-visible behaviour, the person's experience of the existing two-device app wins.
