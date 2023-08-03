use std::collections::HashSet;

use fnv::FnvHashMap;
use float_ord::FloatOrd;
use lazy_static::lazy_static;

use comrak::nodes::NodeValue;
use rake::{KeywordScore, Rake, StopWords};

use crate::helpers::OrderedSet;
use crate::markdown;

pub fn automatic(content: &str) -> Vec<String> {
    let mut tags = OrderedSet::new();
    let mut added_snippet_tag = false;

    let arena = markdown::storage();
    let root = markdown::parse(&arena, &content);

    markdown::visit_code_blocks::<(), _>(
        &root,
        |current_node| {
            if let NodeValue::CodeBlock(ref block) = current_node.data.borrow().value {
                if !block.info.is_empty() {
                    if !added_snippet_tag {
                        tags.insert("snippet".to_owned());
                        added_snippet_tag = true;
                    }

                    let tag = &block.info;
                    if !tags.contains(tag) {
                        tags.insert(tag.clone());
                    }
                }
            }

            Ok(())
        },
        true,
        false
    ).unwrap();

    let mut non_code_content = String::new();
    markdown::visit_non_code_blocks::<std::io::Error, _>(
        &root,
        |current_node| {
            let node_str = markdown::ast_to_string(current_node)?.to_lowercase().replace("`", "");
            non_code_content.push_str(&node_str);
            Ok(())
        }
    ).unwrap();

    let stop_words = StopWords::from(STOP_LIST.clone());
    let take = Rake::new(stop_words);
    let keywords = take.run(&non_code_content);

    let mut word_frequency = FnvHashMap::default();
    keywords.iter().for_each(
        |&KeywordScore { ref keyword, ref score }| {
            if *score > 1.0 {
                for word in keyword.split(" ") {
                    if word.chars().any(|c| c.is_alphabetic()) {
                        *word_frequency.entry(word).or_insert(0.0) += score;
                    }
                }
            }
        }
    );

    let mut word_scores = Vec::from_iter(word_frequency.into_iter());
    word_scores.sort_by_key(|(_, score)| FloatOrd(-*score));
    for (word, score) in word_scores.into_iter().take(3) {
        if score >= 3.0 {
            let tag = word.to_owned();
            if !tags.contains(&tag) {
                tags.insert(tag);
            }
        }
    }

    tags.into_iter().collect()
}

lazy_static! {
    static ref STOP_LIST: HashSet<String> = {
        let content = include_str!("../data/stop_list.txt");
        HashSet::from_iter(content.lines().map(|x| x.to_owned()))
    };
}

#[test]
fn test_automatic1() {
    let tags = automatic(r#"Hello, World!
``` python
xs = list(range(0, 10))
print([x * x for x in xs])
```

``` cpp
#include <iostream>
int main() {
    std::cout << "Hello, World!" << std::endl;
}
```

End of world.
"#);

    assert_eq!(
        vec!["snippet".to_owned(), "python".to_owned(), "cpp".to_owned()],
        tags
    );
}

#[test]
fn test_automatic2() {
    let tags = automatic(r#"Hello, World!
``` python
xs = list(range(0, 10))
print([x * x for x in xs])
```

``` cpp
#include <iostream>
int main() {
    std::cout << "Hello, World!" << std::endl;
}
```

# sqlgrep
Combines SQL with regular expressions to provide a new way to filter and process text files.

## Build
* Requires cargo (https://rustup.rs/).
* Build with: `cargo build --release`
* Build output in `target/release/sqlgrep`

## Example
First, a schema needs to be defined that will transform text lines into structured data:
```
CREATE TABLE connections(
    line = 'connection from ([0-9.]+) \\((.+)?\\) at ([a-zA-Z]+) ([a-zA-Z]+) ([0-9]+) ([0-9]+):([0-9]+):([0-9]+) ([0-9]+)',

    line[1] => ip TEXT,
    line[2] => hostname TEXT,
    line[9] => year INT,
    line[4] => month TEXT,
    line[5] => day INT,
    line[6] => hour INT,
    line[7] => minute INT,
    line[8] => second INT
);
```

If we want to know the IP and hostname for all connections which have a hostname in the file `testdata/ftpd_data.txt` with the table definition above in `testdata/ftpd.txt`  we can do:

```
sqlgrep -d testdata/ftpd.txt testdata/ftpd_data.txt -c "SELECT ip, hostname FROM connections WHERE hostname IS NOT NULL"
```

We can also do it "live" by tailing following the file (note the `-f` argument):

```
sqlgrep -d testdata/ftpd.txt testdata/ftpd_data.txt -f -c "SELECT ip, hostname FROM connections WHERE hostname IS NOT NULL"
```

If we want to know how many connection attempts we get per hostname (i.e. a group by query):

```
sqlgrep -d testdata/ftpd.txt testdata/ftpd_data.txt -c "SELECT hostname, COUNT() AS count FROM connections GROUP BY hostname"
```

See `testdata` folder and `src/integration_tests.rs` for more examples.

# Documentation
Tries to follow the SQL standard, so you should expect that normal SQL queries work. However, not every feature is supported yet.

## Queries
Supported features:
* Where.
* Group by.
* Aggregates.
* Having.
* Inner & outer joins. The joined table is loaded completely in memory.
* Limits.
* Extract(x FROM y) for timestamps.
* Case expressions.

Supported aggregates:
* `count`
* `min`
* `max`
* `sum`
* `avg`
* `stddev`
* `array_agg`

Supported functions:
* `least(INT|REAL, INT|REAL) => INT|REAL`
* `greatest(INT|REAL, INT|REAL) => INT|REAL`
* `abs(INT|REAL) => INT|REAL`
* `sqrt(REAL) => REAL`
* `pow(REAL, REAL) => REAL`
* `regex_matches(TEXT, TEXT) => BOOLEAN`
* `length(TEXT) => INT`
* `upper(TEXT) => TEXT`
* `lower(TEXT) => TEXT`
* `array_unique(ARRAY) => ARRAY`
* `array_length(ARRAY) => INT`
* `array_cat(ARRAY) => ARRAY`
* `array_append(ARRAY, ANY) => ARRAY`
* `array_prepend(ANY, ARRAY) => ARRAY`
* `now() => TIMESTAMP`
* `make_timestamp(INT, INT, INT, INT, INT, INT, INT) => TIMESTAMP`
* `date_trunc(TEXT, TIMESTAMP) => TIMESTAMP`

## Special features
The input file can either be specified using the CLI or as an additional argument to the `FROM` statement as following:
```
SELECT * FROM connections::'file.log';
```

## Tables
### Syntax
```
CREATE TABLE <name>(
    Separate pattern and column definition. Pattern can be used in multiple column definitions.
    <pattern name> = '<regex patern>',
    <pattern name>[<group index>] => <column name> <column type>,

    Use regex splits instead of matches.
    <pattern name> = split '<regex patern>',

    Inline regex. Will be bound to the first group
    '<regex patern>' => <column name> <column type>

    Array pattern. Will create array of fixed sized based on the given patterns.
    <pattern name>[<group index>], <pattern name>[<group index>], ... => <column name> <element type>[],

    Timestamp pattern. Will create a timestamp. Year, month, day, hour, minute, second. Each part is optional.
    <pattern name>[<group index>], <pattern name>[<group index>], ... => <column name> TIMESTAMP,

    Json pattern. Will access the given attribute.
    { .field1.field2 } => <column name> <column type>,
    { .field1[<array index>] } => <column name> <column type>,
);
```
Multiple tables can be defined in the same file.

### Supported types
* `TEXT`: String type.
* `INT`: 64-bits integer type.
* `REAL`: 64-bits float type.
* `BOOLEAN`: True/false type. When extracting data, it means the _existence_ of a group.
* `<element type>[]`: Array types such as `real[]`.
* `TIMESTAMP`: Timestamp type.
* `INTERVAL`: Interval type.

### Modifiers
Placed after the column type and adds additional constraints/transforms when extracting vale for a column.
* `NOT NULL`: The column cannot be `NULL`. If a not null column gets a null value, the row is skipped.
* `TRIM`: Trim string types for whitespaces.
* `CONVERT`: Tries to convert a string value into the value type.
* `DEFAULT <value>`: Use this as default value instead of NULL.
* `MICROSECONDS`: The decimal second part is in microseconds, not milliseconds.
"#);

    assert_eq!(
        vec!["snippet".to_owned(), "python".to_owned(), "cpp".to_owned(), "type".to_owned(), "sql".to_owned(), "supported".to_owned()],
        tags
    );
}
