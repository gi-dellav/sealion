const full_parser = "                                                \
function require(p) {                                                \
    return ! p ? amb() : 'Satisfied require';                        \
}                                                                    \
function member(item, x) {                                           \
    return is_null(x)                                                \
        ? null                                                       \
        : item === head(x)                                           \
          ? x                                                        \
          : member(item, tail(x));                                   \
}                                                                    \
let unparsed = null;                                                 \
function parse_word(word_list) {                                     \
    require(! is_null(unparsed));                                    \
    require(! is_null(member(head(unparsed), tail(word_list))));     \
    const found_word = head(unparsed);                               \
    unparsed = tail(unparsed);                                       \
    return list(head(word_list), found_word);                        \
}                                                                    \
const prepositions = list('prep', 'for', 'to',  'in', 'by', 'with'); \
function parse_prepositional_phrase() {                              \
    return list('prep-phrase',                                       \
                parse_word(prepositions),                            \
                parse_noun_phrase());                                \
}                                                                    \
const nouns = list('noun', 'student', 'professor', 'cat', 'class');  \
const verbs = list('verb', 'studies', 'lectures', 'eats', 'sleeps'); \
                                                                     \
const articles = list('article', 'the', 'a');                        \
function parse_simple_noun_phrase() {                                \
    return list('simple-noun-phrase',                                \
                parse_word(articles),                                \
                parse_word(nouns));                                  \
}                                                                    \
function parse_noun_phrase() {                                       \
    function maybe_extend(noun_phrase) {                             \
        return amb(noun_phrase,                                      \
                   maybe_extend(list('noun-phrase',                  \
                                 noun_phrase,                        \
                                 parse_prepositional_phrase())));    \
    }                                                                \
    return maybe_extend(parse_simple_noun_phrase());                 \
}                                                                    \
function parse_sentence() {                                          \
    return list('sentence',                                          \
                parse_noun_phrase(),                                 \
                parse_verb_phrase());                                \
}                                                                    \
function parse_verb_phrase() {                                       \
    function maybe_extend(verb_phrase) {                             \
        return amb(verb_phrase,                                      \
                   maybe_extend(list('verb-phrase',                  \
                                 verb_phrase,                        \
                                 parse_prepositional_phrase())));    \
    }		                                                     \
    return maybe_extend(parse_word(verbs));                          \
}                                                                    \
function parse_input(input) {                                        \
    unparsed = input;                                                \
    const sent = parse_sentence();                                   \
    require(is_null(unparsed));                                      \
    return sent;                                                     \
}                                                                    ";
