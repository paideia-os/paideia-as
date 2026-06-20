//! Two-phase borrows for method-call receivers.
//!
//! Two-phase borrows allow &mut borrows to be reserved before argument evaluation,
//! such that immutable reads during argument evaluation are allowed, and then the
//! borrow is activated to exclusive access after all arguments are evaluated.
//!
//! This pattern is essential for expressions like `vec.push(vec.len())`, where:
//! - The receiver `vec` is reserved as &mut
//! - During argument evaluation `vec.len()`, immutable borrows of `vec` are allowed
//! - After all arguments are evaluated, the &mut borrow activates to exclusive

use crate::borrow_walker::BorrowWalker;

/// A reservation of a &mut borrow that hasn't activated yet.
///
/// During the reservation phase, immutable reads of the same binding
/// are allowed (for argument evaluation). On activation, the borrow
/// becomes exclusive.
#[derive(Debug)]
pub struct TwoPhaseReservation {
    /// The binding ID being borrowed.
    pub binding: u32,
    /// The region ID in which the borrow is active.
    pub region: u32,
    /// Whether the reservation has been activated to exclusive access.
    activated: bool,
}

impl TwoPhaseReservation {
    /// Returns whether this reservation has been activated.
    #[must_use]
    pub fn is_activated(&self) -> bool {
        self.activated
    }
}

/// Reserves a &mut borrow for two-phase access.
///
/// During the reservation phase, `borrow_immutable` calls on the walker
/// will succeed for the same binding, allowing immutable reads during
/// argument evaluation. The borrow does not yet become active.
///
/// Call `activate_reservation` to promote this to exclusive access.
#[must_use]
pub fn reserve_two_phase_borrow(binding: u32, region: u32) -> TwoPhaseReservation {
    // Phase 4 m6-004 minimum: record the reservation.
    // The walker is not modified here; immutable borrows will succeed
    // during the reservation window because the mutable borrow is not
    // yet recorded as active.
    TwoPhaseReservation {
        binding,
        region,
        activated: false,
    }
}

/// Activates a two-phase reservation to exclusive access.
///
/// Converts the reservation into an active &mut borrow on the walker.
/// After activation, subsequent calls to `borrow_immutable` on the same
/// binding will fail with S0906.
///
/// # Errors
///
/// Returns `Err(diagnostic)` if there are active immutable borrows
/// on the binding (S0906) or an active mutable borrow (S0907).
pub fn activate_reservation(
    walker: &mut BorrowWalker,
    reservation: &mut TwoPhaseReservation,
) -> Result<(), String> {
    if reservation.activated {
        return Err("S0906: Reservation already activated".to_string());
    }

    // Now the &mut borrow becomes active; subsequent reads fail with S0906.
    walker.borrow_mutable(reservation.binding, reservation.region)?;
    reservation.activated = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::borrow_walker::BorrowWalker;

    #[test]
    fn two_phase_reservation_allows_immutable_read_during_window() {
        let mut walker = BorrowWalker::new();
        let _reservation = reserve_two_phase_borrow(1, 100);

        // During reservation, immutable borrows should succeed
        // because the mutable borrow is not yet active on the walker.
        assert!(walker.borrow_immutable(1, 101).is_ok());
        assert!(walker.borrow_immutable(1, 102).is_ok());

        // The walker has no record of the mutable borrow yet
        let active = walker.active_borrows(1).unwrap();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn two_phase_activation_promotes_to_exclusive() {
        let mut walker = BorrowWalker::new();
        let mut reservation = reserve_two_phase_borrow(1, 100);

        // Before activation, no borrows are on the walker
        assert!(walker.active_borrows(1).is_none());
        assert!(!reservation.is_activated());

        // Activate the reservation
        assert!(activate_reservation(&mut walker, &mut reservation).is_ok());
        assert!(reservation.is_activated());

        // After activation, the walker has the mutable borrow
        let active = walker.active_borrows(1).unwrap();
        assert_eq!(active.len(), 1);
    }

    #[test]
    fn two_phase_reservation_then_activation_blocks_further_reads() {
        let mut walker = BorrowWalker::new();
        let mut reservation = reserve_two_phase_borrow(1, 100);

        // Activate the reservation (no existing borrows)
        assert!(activate_reservation(&mut walker, &mut reservation).is_ok());

        // After activation, immutable reads on the same binding fail with S0906
        let result = walker.borrow_immutable(1, 102);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("S0906"));
    }

    #[test]
    fn two_phase_activation_with_existing_immutable_borrow_fails() {
        let mut walker = BorrowWalker::new();
        let mut reservation = reserve_two_phase_borrow(1, 100);

        // Add an immutable borrow to the walker
        assert!(walker.borrow_immutable(1, 101).is_ok());

        // Activation should fail because there's already an immutable borrow
        let result = activate_reservation(&mut walker, &mut reservation);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("S0906"));
        assert!(!reservation.is_activated());
    }
}
