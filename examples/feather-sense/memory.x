/* Memory layout for nRF52840 with Adafruit Bootloader (SoftDevice S140 6.1.1) */
MEMORY
{
  /* Flash: Bootloader+SoftDevice takes first 0x26000 */
  FLASH : ORIGIN = 0x00026000, LENGTH = 872K

  /* RAM: SoftDevice uses first 24KB (0x6000) */
  RAM : ORIGIN = 0x20006000, LENGTH = 232K
}
