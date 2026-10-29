const a1 = make_account(100);
const a2 = make_account(200);
const a3 = make_account(300);

concurrent_execute(
    () => { 
        display(a1("balance"), "Peter balance a1 before");
        display(a2("balance"), "Peter balance a2 before");
        exchange(a1, a2);
        display(a1("balance"), "Peter balance a1 after");
        display(a2("balance"), "Peter balance a2 after");
    },
    () => { 
        display(a1("balance"), "Paul balance a1 before");
        display(a3("balance"), "Paul balance a3 before");
        exchange(a1, a3);
        display(a1("balance"), "Paul balance a1 after");
        display(a3("balance"), "Paul balance a3 after");
    }
);
