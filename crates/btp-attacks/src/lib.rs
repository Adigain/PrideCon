//! Experiment E6: the dual-key and Method A designs, their attacks, and the final design.

pub mod attacks;
pub mod designs;
pub mod game;
pub mod rank;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Design {
    DualKey,
    MethodA,
    Final,
    FinalTrapdoor,
    FinalX0,
}

impl Design {
    pub const ALL: [Design; 5] = [
        Design::DualKey,
        Design::MethodA,
        Design::Final,
        Design::FinalTrapdoor,
        Design::FinalX0,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Design::DualKey => "dual-key (first design)",
            Design::MethodA => "Method A",
            Design::Final => "final (tagged handles)",
            Design::FinalTrapdoor => "final, trapdoor leaked",
            Design::FinalX0 => "final, registered with x = 0",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Attack {
    A1,
    A2,
    A3,
    A4,
    A5,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Game {
    Payload,
    OldBalance,
}
