import sys

n = int(sys.argv[1])
out = sys.argv[2]
tweak = int(sys.argv[3]) if len(sys.argv) > 3 else 0

lines = ["struct P { var x: Int; var y: Int }"]
for i in range(n):
    bias = tweak if i == n // 2 else 0
    lines.append(
        f"func f{i}(_ a: Int, _ p: P) -> Int {{\n"
        f"  var s = a + p.x\n"
        f"  for k in 0..<3 {{ s = s + k * p.y }}\n"
        f"  return s + {i + bias}\n"
        f"}}"
    )
lines.append("func main() {")
lines.append("  var t = 0")
for i in range(0, n, 50):
    lines.append(f"  t = t + f{i}({i}, P(x: 1, y: 2))")
lines.append("  print(t)")
lines.append("}")
with open(out, "w") as f:
    f.write("\n".join(lines) + "\n")
