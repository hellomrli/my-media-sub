const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

// 属性边界必须完整：v2.7.2 的无障碍改造曾把 ` id="field-user-id"` 插进一个含 `=>`
// 的 @input 表达式中间（脚本把属性值里的 `>` 当成了标签结尾），Alpine 表达式被
// 截断成 `.map(v = id=`，整个设置页在浏览器里抛 SyntaxError——而 Node 侧没有任何
// 测试解析 HTML 属性，只有 CI 的真实浏览器 smoke 才发现。这里做结构化断言：
// 每个 `attr="value"` 的闭合引号之后只能是空白、`/` 或 `>`。
const staticDir = path.join(__dirname, '../static');
const partialsDir = path.join(staticDir, 'partials');

function sourceFiles() {
  const files = fs.readdirSync(partialsDir)
    .filter(name => name.endsWith('.html'))
    .map(name => path.join(partialsDir, name));
  files.push(path.join(staticDir, 'index.tmpl.html'));
  return files;
}

test('partials keep every quoted attribute value structurally intact', () => {
  const attribute = /\s([@:a-zA-Z][\w:.-]*)="([^"]*)"(.)/g;
  const problems = [];
  for (const file of sourceFiles()) {
    const source = fs.readFileSync(file, 'utf8');
    for (const match of source.matchAll(attribute)) {
      const [, name, value, next] = match;
      if (!/[\s/>]/.test(next)) {
        const line = source.slice(0, match.index).split('\n').length;
        problems.push(`${path.basename(file)}:${line} ${name}="${value.slice(0, 60)}…" 后面紧跟 ${JSON.stringify(next)}`);
      }
    }
  }
  assert.deepEqual(problems, [], `属性闭合引号后必须是空白、/ 或 >：\n${problems.join('\n')}`);
});

// 同一个根因的另一种表现：箭头函数被拆成 `v = id=`。直接锁住。
test('Alpine expressions never contain a stray id attribute', () => {
  for (const file of sourceFiles()) {
    const source = fs.readFileSync(file, 'utf8');
    assert.equal(source.includes(' = id="'), false, `${path.basename(file)} 含被截断的表达式`);
  }
});
