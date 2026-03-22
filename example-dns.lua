domain "intranet.levandor.io" {
    prefix "postgres" {
        resolve = "192.168.966.13" --TODO! investigate IP based passthrough
    },

    prefix "datalake" {
        resolve = service("tag:datalake")
    }
}