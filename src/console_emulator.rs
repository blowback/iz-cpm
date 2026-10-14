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
}