pub trait ConsoleEmulator {
    fn status(&mut self) -> bool;
    fn read(&mut self) -> u8;
    fn put(&mut self, sequence: Option<String>);
    fn terminated(&self) -> bool;
    /// True once console input has ended for good (stdin at end of file),
    /// so a read can never be answered. The run then ends with exit code 1.
    #[cfg(feature = "videobeast")]
    fn input_ended(&self) -> bool {
        false
    }
    /// Gives the console the CPU's emulated T-state count before each
    /// instruction, so a status check sees the count at its trap and the
    /// console can tell how close together status checks are.
    #[cfg(feature = "videobeast")]
    fn note_cycle_count(&mut self, _t_states: u64) {}
}