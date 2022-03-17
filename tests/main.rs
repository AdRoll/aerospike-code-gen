use aerospike_code_gen::define;

fn main() {
    let _my_func: &str = define! {
        function my_func(rec)
            if true then
                local mmap = {}
            end
        end
    };
}
