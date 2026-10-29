function prompt_for_input(input_prompt) {
    const input = prompt(input_prompt);
    if (is_null(input)) {
        display("--- evaluator terminated ---");
        return null;
    } else {
        display(input_prompt + "\n" + input + "\n----------------------------");
        return parse(input);
    }
}
