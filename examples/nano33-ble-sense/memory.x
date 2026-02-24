/* Memory layout for nRF52840 with SoftDevice S140 */
/* SoftDevice uses first 0x27000 bytes of flash and first 0x20000 of RAM */
MEMORY
{
  /* Flash: 1MB total, SoftDevice takes first 156KB (0x27000) */
  FLASH : ORIGIN = 0x00027000, LENGTH = 868K
  
  /* RAM: 256KB total, SoftDevice takes first 64KB (0x10000) for its use */
  /* We also need to reserve some RAM at the start for SoftDevice */
  RAM : ORIGIN = 0x20010000, LENGTH = 192K
}
