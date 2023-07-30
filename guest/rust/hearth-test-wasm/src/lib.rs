use hearth_guest::{wasm::WasmSpawnInfo, Process, Signal};

fn get_spawner() -> Process {
    Process::get_service("hearth.cognito.WasmProcessSpawner")
        .expect("couldn't find Wasm spawner service")
}

fn send<T: serde::Serialize>(dst: &Process, data: &T) {
    let msg = serde_json::to_string(data).unwrap();
    dst.send(&msg.into_bytes(), &[&hearth_guest::SELF]);
}

fn recv<T: for<'a> serde::Deserialize<'a>>() -> (Vec<Process>, T) {
    let signal = Signal::recv();
    let Signal::Message(msg) = signal else {
        panic!("expected message, received {:?}", signal);
    };

    let data = serde_json::from_slice(&msg.data).unwrap();
    (msg.caps, data)
}

fn spawn(spawner: &Process, cb: fn()) -> Process {
    send(
        spawner,
        &WasmSpawnInfo {
            lump: hearth_guest::this_lump(),
            entrypoint: Some(unsafe { std::mem::transmute::<fn(), usize>(cb) } as u32),
        },
    );

    let signal = Signal::recv();
    let Signal::Message(mut msg) = signal else {
        panic!("expected message, received {:?}", signal);
    };

    msg.caps.remove(0)
}

#[no_mangle]
pub extern "C" fn run() {
    let spawner = get_spawner();
    let input: Vec<u64> = (0..10000000).into_iter().rev().collect();
    merge_sort(&spawner, input);
}

#[derive(serde::Deserialize, serde::Serialize)]
struct Data {
    pub inner: Vec<u64>,
}

fn bubble_sort(input: &mut Vec<u64>) {
    let mut swapped = true;
    while swapped {
        swapped = false;
        for idx in 0..(input.len() - 1) {
            if input[idx] >= input[idx + 1] {
                input.swap(idx, idx + 1);
                swapped = true;
            }
        }
    }
}

fn merge_sort_outer() {
    let (mut caps, input) = recv::<Data>();
    let parent = caps.remove(0);
    let spawner = get_spawner();
    let result = merge_sort(&spawner, input.inner);
    send(&parent, &Data { inner: result });
}

fn merge_sort(spawner: &Process, mut input: Vec<u64>) -> Vec<u64> {
    if input.len() > 32 {
        let a_pid = spawn(spawner, merge_sort_outer);
        let b_pid = spawn(spawner, merge_sort_outer);

        let (a, b) = input.split_at(input.len() / 2);
        send(&a_pid, &Data { inner: a.to_vec() });
        send(&b_pid, &Data { inner: b.to_vec() });

        let (_caps, a_sorted) = recv::<Data>();
        let (_caps, b_sorted) = recv::<Data>();

        merge(a_sorted.inner, b_sorted.inner)
    } else {
        bubble_sort(&mut input);
        input
    }
}

fn merge(mut a: Vec<u64>, b: Vec<u64>) -> Vec<u64> {
    // blech i can do this later
    a.extend_from_slice(&b);
    a.sort_unstable();
    a
}
