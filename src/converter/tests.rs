use super::convert;

fn run(source: &str) -> String {
    let conversion = convert(source);
    assert!(
        conversion.errors.is_empty(),
        "unexpected errors for:\n{source}\n\n{:#?}",
        conversion.errors
    );
    conversion.code
}

fn expected(lines: &[&str]) -> String {
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn check(source: &str, lines: &[&str]) {
    assert_eq!(run(source), expected(lines), "source:\n{source}");
}

#[test]
fn declarations_and_expressions() {
    check(
        r#"
int x = 1;
int y, z = 2;
x = x + y * 2;
y += 3;
z = (x > 0) ? x : -x;
char *msg = "hi";
int arr[10];
"#,
        &[
            "INIT int x TO 1",
            "INIT int y",
            "INIT int z TO 2",
            "SET x TO x + y * 2",
            "INCREMENT y BY 3",
            "SET z TO (x IS GREATER THAN 0) ? x : -x",
            "INIT char *msg TO \"hi\"",
            "INIT int arr[10]",
        ],
    );
}

#[test]
fn conditionals() {
    check(
        r#"
if (a && !b) {
    c = 1;
} else if (a || b) {
    c = 2;
} else {
    c = 3;
}
"#,
        &[
            "IF (a AND NOT b)",
            "    SET c TO 1",
            "ELSE IF (a OR b)",
            "    SET c TO 2",
            "ELSE",
            "    SET c TO 3",
            "ENDIF",
        ],
    );
}

#[test]
fn dangling_else_snippets() {
    check(
        r#"
else if (x > 0) {
    y = 1;
}
"#,
        &["ELSE IF (x IS GREATER THAN 0)", "    SET y TO 1", "ENDIF"],
    );

    check(
        r#"
else if (x > 0) {
    y = 1;
} else {
    y = 2;
}
"#,
        &[
            "ELSE IF (x IS GREATER THAN 0)",
            "    SET y TO 1",
            "ELSE",
            "    SET y TO 2",
            "ENDIF",
        ],
    );

    check(
        r#"
else {
    y = 2;
}
"#,
        &["ELSE", "    SET y TO 2", "ENDIF"],
    );
}

#[test]
fn loops() {
    check(
        r#"
while (i < 10) i++;
for (int i = 0; i < n; i += 2) {
    sum += i;
}
for (;;) {
    break;
}
do {
    x--;
} while (x > 0);
"#,
        &[
            "WHILE (i IS LESS THAN 10)",
            "    INCREMENT i",
            "ENDWHILE",
            "FOR (INIT int i TO 0; i IS LESS THAN n; INCREMENT i BY 2)",
            "    INCREMENT sum BY i",
            "ENDFOR",
            "FOREVER",
            "    BREAK",
            "ENDFOR",
            "DO",
            "    DECREMENT x",
            "WHILE (x IS GREATER THAN 0)",
        ],
    );
}

#[test]
fn functions_and_io() {
    check(
        r#"
int add(int a, int b) {
    return a + b;
}
void greet(void) {
    printf("Hello, %s!\n", "world");
    return;
}
int main(int argc, char **argv) {
    int x = add(1, 2);
    return 0;
}
"#,
        &[
            "FUNCTION add(a, b) RETURNS int",
            "    RETURN a + b",
            "END FUNCTION",
            "",
            "FUNCTION greet()",
            "    PRINT \"Hello, %s!\\n\", \"world\"",
            "    RETURN",
            "END FUNCTION",
            "",
            "FUNCTION main(argc, argv)",
            "    INIT int x TO add(1, 2)",
            "    RETURN 0",
            "END FUNCTION",
        ],
    );
}

#[test]
fn switch_statement() {
    check(
        r#"
switch (op) {
    case 1:
        printf("one");
        break;
    case 2:
    case 3:
        puts("many");
        break;
    default:
        scanf("%d", &x);
}
"#,
        &[
            "SWITCH (op)",
            "CASE 1:",
            "    PRINT \"one\"",
            "    BREAK",
            "CASE 2:",
            "CASE 3:",
            "    PRINT \"many\"",
            "    BREAK",
            "DEFAULT:",
            "    READ x",
        ],
    );
}

#[test]
fn structs_enums_and_typedefs() {
    check(
        r#"
struct Point {
    int x;
    int y;
};
typedef struct Point Point;
enum Color { RED, GREEN = 5 };
struct Point p;
"#,
        &[
            "STRUCT Point",
            "    INIT int x",
            "    INIT int y",
            "END STRUCT",
            "",
            "TYPE Point",
            "",
            "ENUM Color",
            "    RED",
            "    GREEN TO 5",
            "END ENUM",
            "",
            "INIT struct Point p",
        ],
    );
}

#[test]
fn pointers_and_members() {
    check(
        r#"
struct Node {
    int value;
    struct Node *next;
};
void f(struct Node *n, int v) {
    n->value = v;
    (*n).next = NULL;
    n->next->value = v;
}
"#,
        &[
            "STRUCT Node",
            "    INIT int value",
            "    INIT struct Node *next",
            "END STRUCT",
            "",
            "FUNCTION f(n, v)",
            "    SET n->value TO v",
            "    SET (*n).next TO NULL",
            "    SET n->next->value TO v",
            "END FUNCTION",
        ],
    );
}

#[test]
fn comments_and_preprocessor() {
    check(
        r#"
#include <stdio.h>
// line comment
/* block
   comment */
#define LIMIT 10
#define DEBUG
#define SQUARE(x) ((x) * (x))
int main(void) {
    return LIMIT; // trailing
}
"#,
        &[
            "// line comment",
            "/* block",
            "   comment */",
            "",
            "CONSTANT LIMIT TO 10",
            "",
            "CONSTANT DEBUG",
            "",
            "FUNCTION main()",
            "    RETURN LIMIT",
            "    // trailing",
            "END FUNCTION",
        ],
    );
}

#[test]
fn conditional_preprocessor() {
    check(
        r#"
#ifdef DEBUG
int x = 1;
#else
int x = 2;
#endif
#if defined(FOO) && !defined(BAR)
int y = 3;
#endif
#ifndef LIMIT
int z = 4;
#endif
"#,
        &[
            "#IFDEF DEBUG",
            "INIT int x TO 1",
            "#ELSE",
            "INIT int x TO 2",
            "#ENDIF",
            "#IF DEFINED(FOO) && !DEFINED(BAR)",
            "INIT int y TO 3",
            "#ENDIF",
            "#IFNDEF LIMIT",
            "INIT int z TO 4",
            "#ENDIF",
        ],
    );
}

#[test]
fn while_break_continue() {
    check(
        r#"
while (1) {
    if (x == 0) break;
    if (x < 0) continue;
    x--;
}
"#,
        &[
            "WHILE (1)",
            "    IF (x IS EQUAL TO 0)",
            "        BREAK",
            "    ENDIF",
            "    IF (x IS LESS THAN 0)",
            "        CONTINUE",
            "    ENDIF",
            "    DECREMENT x",
            "ENDWHILE",
        ],
    );
}

#[test]
fn chained_else_if_has_one_end_if() {
    let output =
        run("int f(int x) { if (x == 1) return 1; else if (x == 2) return 2; else return 3; }");
    assert_eq!(output.matches("ENDIF").count(), 1);
    assert!(output.contains("ELSE IF (x IS EQUAL TO 2)"));
}

#[test]
fn malformed_input_reports_errors() {
    let conversion = convert("int main( { return 0; }");
    assert!(!conversion.errors.is_empty());
}

#[test]
fn bundled_example_converts_cleanly() {
    let source = include_str!("../../examples/sample.c");
    let conversion = convert(source);
    assert!(conversion.errors.is_empty(), "{:#?}", conversion.errors);
    assert!(
        conversion
            .code
            .contains("FUNCTION factorial(n) RETURNS int")
    );
    assert!(conversion.code.contains("FUNCTION main()"));
    assert!(conversion.code.contains("SWITCH (choice)"));
    assert!(conversion.code.contains("END FUNCTION"));
}

#[test]
fn custom_term_and_text_rules() {
    use super::Rules;
    let (rules, errors) = Rules::from_text(
        "term FUNCTION => PROCEDURE\nterm PRINT => OUTPUT\nterm TO => :=\nterm NULL => NIL\n",
    );
    assert!(errors.is_empty(), "{errors:?}");

    let source = r#"int f(void) { int x = NULL; printf("<- null\n"); return 0; }"#;
    let code = super::convert_with_rules(source, &rules).code;

    assert!(code.contains("PROCEDURE f()"), "{code}");
    assert!(code.contains("END PROCEDURE"), "{code}");
    assert!(code.contains("INIT int x := NIL"), "{code}");
    assert!(code.contains("OUTPUT \"<- null\\n\""), "{code}");
}

#[test]
fn custom_call_rules_expand_arguments() {
    use super::Rules;
    let (rules, errors) = Rules::from_text(
        "call printf => OUTPUT {args}\ncall log_debug => LOG {2}, {1}\ncall my_print => SHOW\n",
    );
    assert!(errors.is_empty(), "{errors:?}");

    let source = r#"
void f(void) {
    printf("x=%d", 42);
    log_debug("tag", value);
    my_print("a", 1);
    puts("hi");
}
"#;
    let code = super::convert_with_rules(source, &rules).code;
    assert!(code.contains("OUTPUT \"x=%d\", 42"), "{code}");
    assert!(code.contains("LOG value, \"tag\""), "{code}");
    assert!(code.contains("SHOW \"a\", 1"), "{code}");
    assert!(code.contains("PRINT \"hi\""), "{code}");
}

#[test]
fn rules_parser_reports_bad_lines() {
    use super::Rules;
    let (rules, errors) =
        Rules::from_text("bogus x => y\nterm A => B\ncall 1bad => X\ntext => Y\n");
    assert_eq!(rules.rule_count(), 1);
    assert_eq!(errors.len(), 3, "{errors:?}");
}

#[test]
fn empty_rules_do_not_change_output() {
    use super::Rules;
    let source = include_str!("../../examples/sample.c");
    let plain = convert(source);
    let with_empty_rules = super::convert_with_rules(source, &Rules::default());
    assert_eq!(plain.code, with_empty_rules.code);
}

#[test]
fn sample_dialect_keeps_casts_comments_and_comparisons() {
    let source = r#"
int f(int *p) {
    /* keep me */
    int x = 0;
    if (*p != 0 && x <= 1) {
        *p = (int)*p + 1;
    }
    switch (x) {
    case 1:
        x++;
        break;
    default:
        x = 0;
    }
    return x;
}
"#;
    let code = run(source);
    assert!(code.contains("INIT int x TO 0"), "{code}");
    assert!(
        code.contains("IF (*p IS NOT EQUAL TO 0 AND x IS LESS THAN OR EQUAL TO 1)"),
        "{code}"
    );
    assert!(code.contains("SET *p TO (int)*p + 1"), "{code}");
    assert!(code.contains("SWITCH (x)"), "{code}");
    assert!(code.contains("CASE 1:"), "{code}");
    assert!(code.contains("INCREMENT x"), "{code}");
    assert!(code.contains("DEFAULT:"), "{code}");
    assert!(code.contains("/* keep me */"), "{code}");
}

#[test]
fn tabs_expand_to_four_spaces() {
    let source = "\t/* a\tb */\nint x = 0;\n";
    let code = run(source);
    assert!(!code.contains('\t'), "{code}");
    assert!(code.contains("/* a    b */"), "{code}");
    assert!(code.contains("INIT int x TO 0"), "{code}");
}

#[test]
fn blank_lines_are_retained() {
    let source = "int a = 0;\n\nint b = 1;\n";
    let code = run(source);
    assert_eq!(code, "INIT int a TO 0\n\nINIT int b TO 1\n");
}

#[test]
fn multiple_blank_lines_are_kept() {
    let source = "int a = 0;\n\n\nint b = 1;\n";
    let code = run(source);
    assert_eq!(code, "INIT int a TO 0\n\n\nINIT int b TO 1\n");
}

#[test]
fn pasted_snippet_indentation_is_retained() {
    let source = "                                 for(i = 0; i < num; i ++) {\n                                     work();\n                                 }\n";
    let code = run(source);
    assert!(
        code.starts_with(
            "                                 FOR (SET i TO 0; i IS LESS THAN num; INCREMENT i)"
        ),
        "{code}"
    );
    assert!(
        code.contains("\n                                     CALL work()"),
        "{code}"
    );
}
