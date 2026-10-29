const t = make_table();
const get = t("lookup");
const put = t("insert");
const print = t("print");

// The test results

put(3, "d");
put(1, "a");
put(2, "b");
put(2, "c");
put(4, "e");
put(5, "f");

print();

display(get(2)); // displays: "c"
