// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// The Mac's mouse and keyboard on this device, as the Mac shares them (its Kvm drives us through
// wireless debugging): where the Mac is (kvm.layout), cursor sharing on/off (kvm.link, the same
// switch as the hotkey's double-tap), and the Mac's hotkey (phone.mac). The rest of the Cursor and
// Keyboard pages are pair.sync switches and values (PairSync).
//
// On a desk with several computers, a handover arrives as kvm.take with a route number
// (specs/001-multi-device-mesh). The phone is a leaf: it answers accept or refuse, and it can take
// control back, but it never hands the pointer on to a third device (MeshLeaf.kt).
package com.glidedesk.android

import org.json.JSONObject

class Kvm(core: Core) : Feature(core) {
    // Where the Mac is from here: left | right | top | bottom.
    val macSide: String get() = core.store.prefsString("kvm.macSide", "right")
    @Volatile var linked = true; private set
    @Volatile var hotkey = "Right Option"; private set

    // The handover state of this phone as a leaf device (Core.mesh): which route, and whether it is
    // driven. One instance on the Core, because the clipboard pool (Clip.kt) uses the same state.
    private val leaf: MeshLeaf get() = core.mesh
    val driven: Boolean get() = leaf.driven
    val route: Long get() = leaf.route

    private val opposite = mapOf("left" to "right", "right" to "left", "top" to "bottom", "bottom" to "top")

    // kvm.layout {pcSide}: where the other side is, from the sender. Ours is the opposite of theirs.
    fun setMacSide(side: String) {
        if (side !in opposite) return
        core.store.setPrefsString("kvm.macSide", side)
        core.send(msg("kvm.layout", "pcSide" to side))
        core.changed()
    }

    fun setLinked(on: Boolean) {
        linked = on
        core.send(msg("kvm.link", "on" to on))
        core.changed()
    }

    // The person took control back here. The driver is told with the route it used, so only the
    // handover in progress ends; a late answer cannot take the pointer away from a newer one.
    fun takeBack() {
        val route = leaf.takeBack() ?: return
        core.send(msg("kvm.home", "route" to route))
        core.changed()
    }

    // kvm.take {route, mouse?, keyboard?}: the computer wants the pointer here.
    //
    // A take with no route comes from a build without the mesh and is answered exactly as before
    // (nothing to answer): its own edge watch brings the pointer back.
    private fun take(m: JSONObject) {
        val route = m.optLong("route", 0L)
        if (route <= 0L) return
        // Absent means wanted, as on the wire: a take names only what it does not want.
        val mouse = if (m.has("mouse")) m.optBoolean("mouse") else true
        val keyboard = if (m.has("keyboard")) m.optBoolean("keyboard") else true
        if (!linked) {
            // Cursor sharing is off on this phone: say why, so the computer stops instead of
            // leaving its pointer hidden on a screen nobody is driving.
            core.send(msg("kvm.refuse", "route" to route, "why" to "not_allowed"))
            core.changed()
            return
        }
        if (!leaf.take(route, mouse, keyboard)) return // an older or repeated take: ignored
        core.send(msg("kvm.accept", "route" to route))
        core.changed()
    }

    override fun onMessage(t: String, m: JSONObject): Boolean {
        when (t) {
            "kvm.layout" -> opposite[m.optString("pcSide")]?.let { core.store.setPrefsString("kvm.macSide", it) }
            "kvm.link" -> linked = m.optBoolean("on", true)
            "phone.mac" -> hotkey = m.optString("hotkey").ifEmpty { hotkey }
            "kvm.take" -> take(m)
            "kvm.release", "kvm.home" -> {
                if (leaf.release(m.optLong("route", 0L))) core.changed()
            }
            "k.mm", "k.md", "k.mu", "k.wh", "k.kd", "k.ku", "k.ge" -> Unit // the Mac injects through adb
            else -> return false
        }
        core.changed()
        return true
    }

    override fun onClosed() {
        if (leaf.driven) core.changed()
        leaf.reset()
    }
}
