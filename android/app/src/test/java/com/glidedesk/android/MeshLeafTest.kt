// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Tests for MeshLeaf: a phone as a leaf of a multi-device desk. It answers a handover, can take
// control back, and never relays a crossing onward (specs/001-multi-device-mesh, FR-015, FR-017,
// FR-018). Run with: ./gradlew test
package com.glidedesk.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class MeshLeafTest {
    @Test
    fun a_newer_take_is_accepted_and_an_older_one_is_ignored() {
        val leaf = MeshLeaf()
        assertFalse("nothing to take before a route", leaf.driven)

        assertTrue(leaf.take(7, mouse = true, keyboard = true))
        assertTrue(leaf.driven)
        assertEquals(7L, leaf.route)

        assertFalse("the same route again is a duplicate", leaf.take(7, mouse = true, keyboard = true))
        assertFalse("a route from before this handover is late", leaf.take(6, mouse = true, keyboard = true))
        assertEquals("the handover in hand is not replaced", 7L, leaf.route)

        assertTrue(leaf.take(9, mouse = false, keyboard = true))
        assertEquals(9L, leaf.route)
        assertFalse("a keyboard-only take asks for no pointer", leaf.wantsMouse)
        assertTrue(leaf.wantsKeyboard)
    }

    @Test
    fun a_release_only_ends_the_handover_in_progress() {
        val leaf = MeshLeaf()
        assertFalse("not driven: nothing to release", leaf.release(4))

        leaf.take(4, mouse = true, keyboard = true)
        assertFalse("a release from an older handover leaves this one alone", leaf.release(3))
        assertTrue(leaf.driven)

        assertTrue(leaf.release(4))
        assertFalse(leaf.driven)
        assertFalse("released twice is not a change", leaf.release(4))
    }

    @Test
    fun taking_control_back_names_the_route_once() {
        val leaf = MeshLeaf()
        assertNull("nothing to give up", leaf.takeBack())

        leaf.take(12, mouse = true, keyboard = true)
        assertEquals("the driver's route comes back, so it can match its own", 12L, leaf.takeBack())
        assertFalse(leaf.driven)
        assertNull("control is already back", leaf.takeBack())
    }

    @Test
    fun a_handover_does_not_survive_the_session() {
        val leaf = MeshLeaf()
        leaf.take(3, mouse = true, keyboard = true)
        leaf.reset()
        assertFalse(leaf.driven)
        assertEquals(0L, leaf.route)
        assertTrue("an ask unset by the take is wanted again", leaf.wantsMouse)
        assertTrue(leaf.wantsKeyboard)
    }

    @Test
    fun a_pooled_copy_is_applied_once_and_only_when_newer() {
        val leaf = MeshLeaf()
        assertTrue("nothing applied yet", leaf.clipIsNewer(1000))
        leaf.clipApplied(1000)
        assertFalse("the same item again", leaf.clipIsNewer(1000))
        assertFalse("an older item that arrived late", leaf.clipIsNewer(999))
        assertTrue("a newer item", leaf.clipIsNewer(1001))
        leaf.clipApplied(1001)
        assertEquals(1001L, leaf.lastAppliedTs)

        assertTrue("a build with no stamp is applied as before", leaf.clipIsNewer(0))
        leaf.clipApplied(0)
        assertEquals("an unstamped item does not move the clock", 1001L, leaf.lastAppliedTs)
    }

    @Test
    fun a_stamp_beyond_the_protocol_maximum_is_capped() {
        val leaf = MeshLeaf()
        val big = MeshLeaf.MAX_TS + 5000
        leaf.clipApplied(big)
        assertEquals(MeshLeaf.MAX_TS, leaf.lastAppliedTs)
        assertFalse("anything at the cap is not newer", leaf.clipIsNewer(big))
    }

    // The phone is a leaf: it must never hand the pointer to a third device. Asserted on the wire
    // side too, because a stray string is all it would take.
    @Test
    fun the_phone_never_relays_a_crossing() {
        for (name in listOf("Kvm.kt", "MeshLeaf.kt")) {
            val text = source(name)
            assertFalse("$name must never send kvm.next", text.contains("kvm.next"))
        }
        assertEquals(
            "the phone has no kvm.next to send",
            0,
            Proto.T.javaClass.declaredFields.count { it.name.equals("KVM_NEXT", ignoreCase = true) },
        )
    }

    // The unit-test working directory is the module (android/app); accept either layout.
    private fun source(name: String): String {
        val relative = "src/main/java/com/glidedesk/android/$name"
        val file = listOf(File(relative), File("app/$relative")).firstOrNull { it.exists() }
        assertTrue("$name not found from ${File(".").absolutePath}", file != null)
        return file!!.readText()
    }
}
