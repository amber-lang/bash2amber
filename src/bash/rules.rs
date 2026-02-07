use heraclitus_compiler::prelude::*;

pub fn get_rules() -> Rules {
    let symbols = vec![';', '|', '&'];
    let compounds = vec![('&', '&'), ('|', '|')];
    let region = reg![
        reg!(single_quote as "single quoted string" => {
            begin: "'",
            end: "'"
        }),
        reg!(double_quote as "double quoted string" => {
            begin: "\"",
            end: "\""
        }),
        reg!(backticks as "backtick command" => {
            begin: "`",
            end: "`"
        }),
        reg!(comment as "comment" => {
            begin: "#",
            end: "\n",
            allow_unclosed_region: true,
            ignore_escaped: true
        })
    ];
    Rules::new(symbols, compounds, region)
}
