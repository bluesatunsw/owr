MEMORY
{
  /* NOTE 1 K = 1 KiBi = 1024 bytes */
  /* The Nucleo is an RET6, so has 512K flash.
   * All actual rover boards use the RBT6, so 128K.
   * We'll use the minimum. */
  FLASH : ORIGIN = 0x08000000, LENGTH = 128K
  RAM : ORIGIN = 0x20000000, LENGTH = 128K
}
