/* Memory layout for nRF52840 with SoftDevice S140 6.1.1
 *
 * SoftDevice occupies: 0x00000000 - 0x00026000 (flash)
 *                      0x20000000 - 0x20006000 (RAM)
 *
 * Application starts at 0x26000 (flash) and 0x20006000 (RAM).
 * Use with UF2 bootloader (base address 0x26000).
 */
MEMORY
{
  /* Flash after SoftDevice: 1024K - 152K = 872K */
  FLASH : ORIGIN = 0x00026000, LENGTH = 872K

  /* RAM after SoftDevice: 256K - 24K = 232K */
  RAM : ORIGIN = 0x20006000, LENGTH = 232K
}
