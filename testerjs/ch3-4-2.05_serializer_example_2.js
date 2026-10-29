// chapter=3 variant=concurrent 
function make_mutex() {
    const cell = list(false);
    function the_mutex(m) {
        return m === "acquire"
               ? test_and_set(cell)
                 ? the_mutex("acquire") // retry
                 : true
               : m === "release"
               ? clear(cell)
               : error(m, "unknown request -- mutex");
    }
    return the_mutex;
}
function clear(cell) {
    set_head(cell, false);
}
function make_serializer() {
    const mutex = make_mutex();
    return f => {
               function serialized_f() {
                   mutex("acquire");
                   const val = f();
                   mutex("release");
                   return val;
               }
               return serialized_f;          
           };
}
let x = 10;

const s = make_serializer();

concurrent_execute(  () => { x = s(() => x * x)(); },
                   s(() => { x = x + 1;             }));

// expected: 'all threads terminated'
