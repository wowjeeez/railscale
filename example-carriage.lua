local machine = service("windows", "tag:server")


spoof_ua.header("User-Agent", "SPOOFED")
spoof_ua.stamp("ua spoofed to SPOOFED")

machine.send(spoof_ua)

-- these do the same


service "spoof-ua" {
    upstream = service("windows", "tag:server"),
    select = https(8080).header("X-Spoof-UA"),
    matched = peer("100.1.3.03") {
        redirect_to = "something-else",
        full = false -- whether to actually re send the whole request against the tailscale service or if local redir is enough

    },
    apply = {
        header("User-Agent", "SPOOFED"),
        stamp("ua spoofed to SPOOFED")
    },
    forward
}

service "something-else" {

    --...
}