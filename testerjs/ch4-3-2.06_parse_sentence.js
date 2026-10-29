// chapter=3 variant=non-det 
const nouns = list("noun", "student", "professor", "cat", "class");

const verbs = list("verb", "studies", "lectures", "eats", "sleeps");

const articles = list("article", "the", "a");
let not_yet_parsed = null;
function parse_word(word_list) {      
    require(! is_null(not_yet_parsed));
    require(! is_null(member(head(not_yet_parsed), tail(word_list))));
    const found_word = head(not_yet_parsed);
    not_yet_parsed = tail(not_yet_parsed);
    return list(head(word_list), found_word);
}
function parse_noun_phrase() {
    return list("noun-phrase",
                parse_word(articles),
                parse_word(nouns));
}
function parse_input(input) {
    not_yet_parsed = input;
    const sent = parse_sentence();
    require(is_null(not_yet_parsed));
    return sent;
}
function parse_sentence() {      
    return list("sentence",
                parse_noun_phrase(),
                parse_word(verbs));
}

parse_input(list("the",  "cat",  "eats"));

// expected: [ "sentence", [ ["noun-phrase", [["article", ["the", null]], [["noun", ["cat", null]], null]]], [["verb", ["eats", null]], null]]]
