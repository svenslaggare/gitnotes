#!/bin/bash
rm -rf .git/ *.md *.metadata ; git init

EXE=../target/debug/gitnotes-cli
$EXE add 2022/05/test1 --tags x y <<EOF
Hello, World!

\`\`\` python
import numpy as np
print(np.square(np.arange(0, 10)))
\`\`\`
EOF

echo "Hello, Stupid World!" | $EXE add 2023/05/test2 --tags x z
echo "Hello, New World!" | $EXE add 2023/test3 --tags x y