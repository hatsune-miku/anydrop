import { parseGroup } from '../src/model'

test('preserves all uint32 channel values without rounding', () => {
  expect(parseGroup('0')).toBe(0)
  expect(parseGroup('4294967295')).toBe(4294967295)
  for (const invalid of ['', '-1', '1.5', '4294967296', 'abc', '1e2', ' 12']) {
    expect(() => parseGroup(invalid)).toThrow()
  }
})
