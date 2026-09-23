package com.anydrop.mobile
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.charset.CodingErrorAction

internal object TextWire {
  const val MAX_BYTES = 4 * 1024 * 1024
  fun intLE(bytes: ByteArray, offset: Int): Int = ByteBuffer.wrap(bytes, offset, 4).order(ByteOrder.LITTLE_ENDIAN).int
  private fun shortLE(bytes: ByteArray, offset: Int): Int = ByteBuffer.wrap(bytes, offset, 2).order(ByteOrder.LITTLE_ENDIAN).short.toInt() and 65535
  private fun hash(text: String): Int { var sum = 65526L; text.codePoints().toArray().forEachIndexed { index, scalar -> sum = (sum + index.toLong() * scalar) and 65535 }; return sum.toInt() }
  fun encode(text: String): ByteArray {
    val bytes = text.toByteArray(Charsets.UTF_8); require(bytes.isNotEmpty() && bytes.size <= MAX_BYTES) { "文本必须为 1 字节到 4 MB" }
    val length = bytes.size + 6
    return ByteBuffer.allocate(length + 12).order(ByteOrder.LITTLE_ENDIAN).putInt(length + 8).putShort(0x3940.toShort()).putInt(length)
      .putInt(bytes.size).put(bytes).putShort(hash(text).toShort()).putShort((length / 2).toShort()).array()
  }
  fun decode(bytes: ByteArray): String {
    require(bytes.size >= 18 && intLE(bytes, 0) == bytes.size - 4 && shortLE(bytes, 4) == 0x3940)
    val length = intLE(bytes, 6); val count = intLE(bytes, 10)
    require(count in 0..MAX_BYTES && length == count + 6 && bytes.size == length + 12 && shortLE(bytes, bytes.size - 2) == ((length / 2) and 65535))
    val text = Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(bytes, 14, count)).toString()
    require(shortLE(bytes, 14 + count) == hash(text)); return text
  }
}
