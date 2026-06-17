object "PastHeapConstReturn" {
  code { let s := datasize("PastHeapConstReturn_deployed") codecopy(0, dataoffset("PastHeapConstReturn_deployed"), s) return(0, s) }
  object "PastHeapConstReturn_deployed" {
    code {
      // Return a constant 32-byte range starting exactly at the heap end (heap is 128 KiB = 0x20000),
      // with no prior store. The inline unchecked seal_return would read one-past-heap into the
      // returndata (information leak); it must instead trap via the bounds-checked path.
      return(0x20000, 0x20)
    }
  }
}
