module.exports = {
  printWidth: 120, // 超过最大值换行
  tabWidth: 2, // 缩进字节数
  useTabs: false, // 缩进不使用tab，使用空格
  singleQuote: true, // 字符串单引号
  semi: false, // 句尾添加分号
  trailingComma: 'es5', // 在对象或数组最后一个元素后面是否加逗号（在ES5中加尾逗号）
  bracketSpacing: true, // 在对象，数组括号与文字之间加空格 "{ foo: bar }"
  // jsxBracketSameLine: false, // 在jsx中把'>' 是否单独放一行
  bracketSameLine: false, // 在jsx中把'>' 是否单独放一行
  arrowParens: 'always', //  (x) => {} 箭头函数参数只有一个时是否要有小括号。avoid：省略括号
  endOfLine: 'auto', // 结尾是 \n \r \n\r auto
  plugins: ['@trivago/prettier-plugin-sort-imports'],
  importOrder: [
    '^react(-.*)?$', // 1. react 相关
    '^[a-zA-Z]', // 2. 其他第三方库 字母开头
    '^@', // 3. @/ 开头的 alias路径包
    '^@[a-zA-Z]', // 4. @任意字母
    '^\\.\\/(?!.*\\.(less|css)$).*', // 5. 相对路径 ./（排除 .less）
    '^\\.\\.\\/(?!.*\\.(less|css)$).*', // 6. 相对路径 ../（排除 .less）
    '\\.(less|css)$', // 7. .less 文件
  ],
  importOrderSeparation: true, // 分组间添加空行
  importOrderSortSpecifiers: true, // 排序导入项
  importOrderParserPlugins: ['typescript', 'jsx', 'decorators-legacy'], // Add parser plugins for better TS support
  // Add parser configuration to ensure TypeScript is handled correctly
  overrides: [
    {
      files: '*.{ts,tsx}',
      options: {
        parser: 'typescript',
      },
    },
  ],
}
