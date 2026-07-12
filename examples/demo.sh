#!/usr/bin/env aster
# demo.aster — AsterShell feature showcase
# Run: source examples/demo.aster

echo "=== AsterShell Feature Demo ==="
echo ""

# --- 1. Basic expansions ---
echo "--- Command Substitution ---"
echo "User: $(whoami)"
echo "Date: $(date +%Y-%m-%d)"
echo "Kernel: $(uname -r)"
echo ""

# --- 2. Arithmetic ---
echo "--- Arithmetic Expansion ---"
echo "2 + 3 = $((2 + 3))"
echo "10 * 5 = $((10 * 5))"
echo "100 / 7 = $((100 / 7))"
echo ""

# --- 3. Parameter expansion ---
echo "--- Parameter Expansion ---"
NAME="aster"
echo "Name: $NAME"
echo "Uppercase: ${NAME^^}"
echo "Length: ${#NAME}"
echo "Default: ${MISSING:-not set}"
echo "Assign default: ${MISSING2:=fallback}"
echo "Remove suffix: ${NAME%r}"
echo "Substitute: ${NAME/a/A}"
echo ""

# --- 4. Brace expansion ---
echo "--- Brace Expansion ---"
echo "Letters: {a..e}"
echo "Numbers: {1..5}"
echo "Padding: {01..05}"
echo "Combinations: {red,green,blue}-{on,off}"
echo "Nested: {a,b}{1,2,3}"
echo ""

# --- 5. Glob patterns ---
echo "--- Glob Expansion ---"
echo "Current dir: $(pwd)"
echo "Rust files: $(ls *.rs 2>/dev/null | wc -w)"
echo ""

# --- 6. Loops ---
echo "--- For Loop ---"
for color in red green blue; do
    echo "Color: $color"
done
echo ""

# --- 7. Conditionals ---
echo "--- Conditionals ---"
X=42
if [ $X -gt 10 ]; then
    echo "$X is greater than 10"
elif [ $X -gt 0 ]; then
    echo "$X is positive"
else
    echo "$X is zero or negative"
fi
echo ""

# --- 8. Pipes and redirects ---
echo "--- Pipes ---"
echo -e "alpha\nbeta\ngamma" | sort -r
echo ""

# --- 9. Heredoc ---
echo "--- Here-Document ---"
cat <<EOF
This is a heredoc.
Line 2: $(date)
Line 3: $((1 + 1)) = 2
EOF
echo ""

# --- 10. Functions ---
echo "--- Functions ---"
greet() {
    local name=$1
    echo "Hello, $name!"
}
greet "World"
greet "AsterShell"
echo ""

# --- 11. Array-like patterns ---
echo "--- Combinatorial Expansion ---"
echo "Months: {Jan,Feb,Mar}"
echo "Days: {Mon,Tue,Wed}"
echo "Pairs: {Jan,Feb,Mar}-{Mon,Tue,Wed}"
echo ""

echo "=== Demo Complete ==="
