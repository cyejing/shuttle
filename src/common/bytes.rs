
#[macro_export]
macro_rules! read_exact {
    ($stream: expr, $array: expr) => {{
        let mut x = $array;
        //        $stream
        //            .read_exact(&mut x)
        //            .await
        //            .map_err(|_| io_err("lol"))?;
        $stream.read_exact(&mut x).await.map(|_| x)
    }};
}
