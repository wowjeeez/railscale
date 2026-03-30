<p align="center">
  <img src="logo.png" alt="Railscale" width="200">
</p>

# Railscale
Insanely fast (really) streaming proxy/dns/packet capture/maybe even firewall service.
For now it supports http header remapping and routing.

# Architecture
Railscale is very complex compared to what one would expect from a proxy. The reason being is that railscale doesnt buffer requests in memory, and is tuned for maximum performance.
The architecture is also designed to be extensible for basically any protocol. 
The mental model i had while writing is:
- todo mermaid chart etc