/* Memory layout for nRF52840 - Pure Rust (no SoftDevice)
 *
 * This layout uses full flash and RAM, starting at 0x0.
 * Requires flashing via probe-rs (SWD debug probe).
 *
 * For SoftDevice builds, use memory-softdevice.x instead.
 */
MEMORY
{
  /* Full 1MB flash */
  FLASH : ORIGIN = 0x00000000, LENGTH = 1024K

  /* Full 256KB RAM */
  RAM : ORIGIN = 0x20000000, LENGTH = 256K
}
