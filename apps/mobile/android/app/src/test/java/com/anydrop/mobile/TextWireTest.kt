package com.anydrop.mobile

import org.junit.Assert.*
import org.junit.Test

class TextWireTest {
  @Test fun desktopFrames() {
    val samples = listOf(
      "AnyDrop" to "1500000040390d00000007000000416e7944726f70b5080600",
      "\u4f60\u597d\n\u540c\u9891\u6bb5\u6587\u672c" to "2400000040391c00000016000000e4bda0e5a5bd0ae5908ce9a291e6aeb5e69687e69cacd6050e00",
      "e\u0301 \ud83d\udc69\u200d\ud83d\udcbb \ud83d\ude00" to "2200000040391a0000001400000065cc8120f09f91a9e2808df09f92bb20f09f98800de30d00",
      "\u0000\t\r\n" to "1200000040390a0000000400000000090d0a37000500",
      "a" to "0f0000004039070000000100000061f6ff0300"
    )
    for ((text, hex) in samples) {
      val frame = TextWire.encode(text)
      assertEquals(hex, frame.joinToString("") { "%02x".format(it.toInt() and 255) })
      assertEquals(text, TextWire.decode(frame))
      for (size in 0 until frame.size) assertThrows(Exception::class.java) { TextWire.decode(frame.copyOf(size)) }
      for (index in listOf(0, 4, 6, 10, frame.lastIndex)) { val damaged = frame.copyOf(); damaged[index] = (damaged[index].toInt() xor 0x80).toByte(); assertThrows(Exception::class.java) { TextWire.decode(damaged) } }
    }
  }
  @Test fun unicodeAndSizeLimit() {
    val invalidUTF8 = TextWire.encode("a"); invalidUTF8[14] = 0xff.toByte()
    assertThrows(Exception::class.java) { TextWire.decode(invalidUTF8) }
    val samples = listOf("你好\n同频段文本", "e\u0301 👩‍💻 😀", "\u0000\t\r\n", "a".repeat(65535))
    for (text in samples) assertEquals(text, TextWire.decode(TextWire.encode(text)))
    assertThrows(Exception::class.java) { TextWire.encode("a".repeat(65536)) }
    assertThrows(Exception::class.java) { TextWire.encode("") }
  }
}
