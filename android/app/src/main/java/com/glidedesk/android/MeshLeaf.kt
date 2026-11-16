// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// The phone's half of a multi-device desk: what it does with a handover, and with a pooled copy.
//
// A phone is a leaf here (specs/001-multi-device-mesh, FR-015): a computer can drive it and it can
// take control back, but it never relays a crossing to a third device. That is why this class has
// no way to hand a pointer on and no way to dial anyone: the session it is on is the only one it
// has, and the device holding the pointer talks to the next one itself.
//
// Pure Kotlin on purpose: no Android types, so the rules are unit-tested on the JVM (MeshLeafTest)
// the same way the Rust and JavaScript cores are. `Kvm.kt` only moves the answers onto the wire.
package com.glidedesk.android

class MeshLeaf {
    companion object {
        /// The largest stamp the protocol allows (JavaScript's safe integer).
        const val MAX_TS = 1L shl 53
    }

    /// The handover this phone is in: the computer's route number, 0 when none.
    @Volatile var route = 0L
        private set

    /// True while a computer is driving this phone.
    @Volatile var driven = false
        private set

    /// Whether that handover asked for the pointer, and for keys. A keyboard-only take leaves the
    /// pointer where the person left it, exactly as it does between two computers.
    @Volatile var wantsMouse = true
        private set
    @Volatile var wantsKeyboard = true
        private set

    /// The newest pooled copy applied here, so a later message of an older (or the same) copy is
    /// not applied a second time and cannot replace a newer one (FR-017, FR-018).
    @Volatile var lastAppliedTs = 0L
        private set

    /// A `kvm.take`. True when it is a new handover and should be accepted.
    ///
    /// A route at or below the one already in hand is a duplicate or a message that arrived late
    /// (`TAKE_TIMEOUT_MS` is short); it changes nothing, and answering it would only confuse the
    /// computer about which handover the phone is in.
    fun take(route: Long, mouse: Boolean, keyboard: Boolean): Boolean {
        if (route <= this.route) return false
        this.route = route
        driven = true
        wantsMouse = mouse
        wantsKeyboard = keyboard
        return true
    }

    /// `kvm.release` from the driver, or `kvm.home` when it gave the pointer up.
    ///
    /// Only the handover in progress can end: a release for a route this phone has already left
    /// behind (or has not taken yet) must not clear a newer one.
    fun release(route: Long): Boolean {
        if (!driven) return false
        if (route < this.route) return false
        driven = false
        return true
    }

    /// The person took control back here. The route to name in `kvm.home`, or null when there is
    /// nothing to give up (the phone is not being driven).
    ///
    /// The driver compares this route with the one it used, so the answer can only end the handover
    /// it belongs to (src-tauri/src/mesh/session.rs, `home_from_driven`).
    fun takeBack(): Long? {
        if (!driven) return null
        driven = false
        return route
    }

    /// True when a copy that arrived should be applied: anything stamped later than the newest copy
    /// already here, and anything from a build that sends no stamp at all (applied as before).
    ///
    /// Synchronised because pictures are assembled off the main thread: two copies landing at once
    /// must not both pass the test, and the newer stamp must be the one that stays.
    @Synchronized
    fun clipIsNewer(ts: Long): Boolean {
        val t = ts.coerceIn(0L, MAX_TS)
        if (t == 0L) return true
        return t > lastAppliedTs
    }

    /// Note a copy as applied here. Called only when it was, so the next one can be compared.
    @Synchronized
    fun clipApplied(ts: Long) {
        val t = ts.coerceIn(0L, MAX_TS)
        if (t > lastAppliedTs) lastAppliedTs = t
    }

    /// The session ended: no handover survives it, and a reconnect starts from nothing. The applied
    /// copy stamp is kept: it is about the clipboard on this device, not about the session.
    fun reset() {
        route = 0L
        driven = false
        wantsMouse = true
        wantsKeyboard = true
    }
}
