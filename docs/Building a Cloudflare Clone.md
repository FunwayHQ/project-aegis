

# **Architecting Resilience: A Post-Mortem Analysis of the November 2025 Cloudflare Outage and a Blueprint for Next-Generation Distributed Edge Infrastructure**

## **1\. Executive Summary and Strategic Imperative**

The digital ecosystem experienced a seismic disruption on November 18, 2025, when Cloudflare, a dominant provider of internet infrastructure, suffered a catastrophic global outage.1 For approximately six hours, vast segments of the modern web—ranging from social media platforms like X (formerly Twitter) to critical AI services like OpenAI’s ChatGPT and ChatGPT Enterprise—were rendered inaccessible.1 The incident was not precipitated by a sophisticated nation-state cyberattack or a physical severance of subsea cables. Rather, it was the result of a "latent bug" within a Bot Management configuration file that grew exponentially in size, triggering a cascading failure across the provider's control and data planes.3

This event serves as a stark validation of the risks associated with infrastructure monoculture. When a single entity mediates nearly 20% of global web traffic 5, its failure modes become systemic risks for the global economy. The user query—requesting a roadmap to replicate Cloudflare’s functionality—is therefore not merely a technical exercise in software selection; it is a strategic mandate to architect a **Distributed Edge Network** capable of surviving the specific failure modes that crippled the incumbent.

This report delivers an exhaustive architectural blueprint for building a resilient, high-performance Content Delivery Network (CDN) and security edge. It synthesizes the lessons learned from the November 2025 outage to propose a system that prioritizes **static stability**, **memory safety**, and **fault isolation**. By leveraging modern paradigms such as the Rust-based Pingora framework 6, kernel-level eBPF filtering 7, and conflict-free replicated data types (CRDTs) for state management 8, we can construct an infrastructure that avoids the fragility of legacy C-based stacks and monolithic control planes.

## **2\. Deconstructing the November 18, 2025 Outage**

To engineer a superior system, one must first perform a rigorous forensic analysis of the incumbent's failure. The outage on November 18 offers a masterclass in how tightly coupled systems and unchecked configuration propagation can lead to "thundering herd" scenarios and global deadlock.

### **2.1 The Latent Bug and Configuration Bloat**

The precipitating event was routine: a scheduled update to the Bot Management configuration. According to Cloudflare’s post-mortem, the configuration file in question was automatically generated. Due to a logic error in the generation script, the file size expanded well beyond expected parameters.3 This oversized file was not rejected by the validation layer; instead, it was pushed to the global fleet of edge servers.

When the edge software attempted to load this massive configuration, a "latent bug" in the parsing logic was triggered. This bug likely involved resource exhaustion or a memory allocation failure, causing the service process to crash.3 Because the system was designed to fail-secure (blocking traffic rather than failing open), widely distributed 500 Internal Server Errors began appearing globally at approximately 11:48 UTC.1

**Architectural Insight:** The critical failure here was not the bug itself—bugs are inevitable in complex software—but the **lack of graceful degradation** and **insufficient input validation**. A resilient system must enforce strict bounds on configuration objects and, crucially, retain the "Last Known Good" state when a new configuration fails to load.10 The system prioritized consistency of the new config over the availability of the service.

### **2.2 The Cascading Control Plane Failure**

Perhaps the most alarming aspect of the outage was the simultaneous unavailability of the Cloudflare Dashboard and API.1 Customers attempting to log in to disable the Bot Management feature or reroute their traffic found themselves locked out. This indicates a perilous lack of separation between the **Data Plane** (traffic processing) and the **Control Plane** (management interfaces).

In a properly segmented architecture, the Control Plane should remain accessible even if the Data Plane is completely effectively offline. The fact that the outage impacted the dashboard suggests shared infrastructure dependencies—perhaps the dashboard itself sits behind the same edge network that was failing, creating a circular dependency that made remediation impossible without backend intervention.11

### **2.3 The Maintenance Coincidence**

Complicating the diagnosis was a concurrent scheduled maintenance event in the Santiago (SCL) data center.1 While Cloudflare ultimately attributed the outage to the configuration bug, the coincidence highlights the difficulty of root cause analysis in distributed systems. Traffic rerouting from Santiago could have exacerbated the load on neighboring nodes just as they were struggling with the configuration crash, potentially accelerating the spread of the failure through the network.10

**Table 1: Failure Analysis and Architectural Countermeasures**

| Failure Mode | November 18 Incident Manifestation | Proposed Architectural Countermeasure |
| :---- | :---- | :---- |
| **Config Propagation** | Oversized file crashed global fleet synchronously.3 | **Canary Deployments:** Roll out configs to 1% of nodes first.14 **Input Validation:** Hard limits on config file size. |
| **Failure State** | System failed closed (500 errors).2 | **Static Stability:** Fail open or revert to Last Known Good state upon parse error.15 |
| **Control Plane** | Dashboard/API became inaccessible.1 | **Plane Separation:** Control plane must host on independent infrastructure, not behind the edge it manages.12 |
| **Software Resilience** | C/C++ memory bug triggered crash.16 | **Memory Safety:** Use Rust (Pingora) to eliminate memory safety classes of bugs.6 |

## **3\. Network Layer Architecture: The Foundation of Availability**

The physical and logical foundation of any CDN is the network. To replicate the reach and resilience of a major provider, we must implement a Border Gateway Protocol (BGP) Anycast architecture. This allows the same IP address to be advertised from multiple geographic locations simultaneously, automatically routing users to the nearest data center in terms of network hops.17

### **3.1 BGP Anycast Strategy and Topology**

Anycast is the primary mechanism for load balancing at the global scale and provides the first line of defense against volumetric Distributed Denial of Service (DDoS) attacks. By advertising the same IP prefix from London, New York, and Tokyo, attack traffic is "sharded" across the global infrastructure rather than concentrating on a single point.17

However, simple Anycast is insufficient. The routing logic must be intelligent enough to handle "route flapping" and stabilize paths. We require a robust routing daemon capable of handling full internet routing tables and applying complex filtering policies.

#### **Technology Selection: BIRD Internet Routing Daemon**

We will utilize **BIRD (BIRD Internet Routing Daemon)**, specifically version 2, which supports both IPv4 and IPv6 in a single configuration channel.18 BIRD is the industry standard for Internet Exchange Points (IXPs) due to its programmable filter language, which is far more flexible than the configuration stanzas of older daemons like Quagga.19

Implementation Detail:  
Each edge node will run BIRD to establish BGP sessions with upstream transit providers and peering partners. The configuration will leverage BIRD's ability to interact with the kernel routing table, injecting routes only when the local health checks pass. If the application layer (the proxy) fails, BIRD must withdraw the route advertisement immediately to shift traffic to the next available data center.20

### **3.2 Network Automation and Source of Truth**

A major risk in managing BGP sessions manually is "fat-finger" errors, where an engineer accidentally advertises a prefix they do not own (route hijacking) or creates a route leak. To mitigate this, we must treat the network configuration as code, driven by a centralized "Source of Truth."

#### **Technology Selection: Peering Manager**

We will deploy **Peering Manager**, an open-source tool designed specifically to manage BGP sessions and document internet exchange points.22 Peering Manager acts as the authoritative database for:

1. **Peering Partners:** Storing ASN details and contact info.  
2. **BGP Sessions:** Tracking the state of sessions (IPv4/IPv6) and policy.  
3. **Configuration Generation:** It can automatically generate BIRD configuration files based on templates (Jinja2), ensuring consistency across thousands of edge nodes.23

This automation is critical. In the Cloudflare incident, the rapid propagation of a bad file caused the outage. In the network layer, a bad BGP configuration can cause the entire internet to "lose" the CDN. By generating configs via Peering Manager and validating them before deployment, we introduce a safety barrier.25

### **3.3 Routing Security: RPKI Validation**

To ensure our clone is a "good citizen" of the internet and to protect our own IP space from being hijacked, we must implement Resource Public Key Infrastructure (RPKI) validation. This cryptographically verifies that an Autonomous System (AS) is authorized to advertise a specific IP prefix.

#### **Technology Selection: Routinator**

We will integrate **Routinator**, an open-source RPKI Relying Party software developed by NLnet Labs.26 Routinator connects to the global RPKI trust anchors, downloads the cryptographic objects, and validates them. It then feeds this validated data to the BIRD daemon via the RTR (RPKI-to-Router) protocol.

**Why Routinator?** It is written in **Rust**, aligning with our broader architectural strategy of prioritizing memory safety.26 Unlike older C-based validators, Routinator is less susceptible to memory corruption vulnerabilities that could be exploited to crash the routing layer.

## **4\. The Data Plane: The Shift to Memory Safety**

The core function of the CDN is the "Data Plane"—the software that accepts incoming connections, terminates TLS, processes HTTP requests, and proxies them to the origin. Historically, Nginx has been the undisputed king of this layer. However, the November 2025 outage, along with years of operational experience at Cloudflare, has exposed the limitations of Nginx's architecture.16

### **4.1 The Obsolescence of Nginx for Modern Edge Scale**

Nginx utilizes a multi-process architecture. A master process spawns several worker processes, each pinning to a CPU core. While efficient for simple file serving, this model struggles in complex edge environments:

1. **Connection Reuse:** Nginx workers cannot easily share connection pools. If Worker A has an open connection to an origin server, Worker B cannot use it. This leads to higher latency and resource churn.27  
2. **Blocking Operations:** Nginx relies on an event loop. If a third-party module (like a WAF or script) performs a blocking operation, it stalls the entire worker process, delaying all other requests assigned to that core.16  
3. **Memory Safety:** Nginx is written in C. As seen in the Cloudflare outage analysis, "latent bugs" in C code (buffer overflows, null pointer dereferences) often lead to hard crashes.16

### **4.2 The Solution: Pingora and the River Proxy**

To overcome these limitations, Cloudflare developed and subsequently open-sourced **Pingora**, a Rust-based framework for building network services.6 Pingora handles over a quadrillion requests daily and was specifically designed to replace Nginx.6

#### **Architectural Advantages of Pingora:**

1. **Thread-based Architecture:** Unlike Nginx's process model, Pingora uses multi-threading with work-stealing. This allows for seamless connection reuse across all threads, significantly reducing the time-to-first-byte (TTFB) and backend load.27  
2. **Memory Safety:** Being written in Rust, Pingora guarantees memory safety at compile time. It eliminates the class of bugs that likely caused the November 18 crash, such as memory corruption during config parsing.29  
3. **Zero-Downtime Upgrades:** Pingora supports graceful restarts where the new binary takes over the listening socket while the old one finishes processing active requests, ensuring no dropped connections during updates.28

#### **Implementation Strategy: River**

Pingora itself is a library, not a turnkey server. To build a usable clone, we will utilize **River**, an open-source reverse proxy built on top of the Pingora library.30 River provides the necessary "glue" code—configuration management, TLS certificate handling, and logging—turning the Pingora engine into a deployable application comparable to Nginx or Apache, but with the performance and safety of Rust.

**Table 2: Comparative Analysis of Proxy Architectures**

| Feature | Nginx (Legacy) | Pingora/River (Modern) | Impact on Resilience |
| :---- | :---- | :---- | :---- |
| **Language** | C | Rust | Rust prevents memory corruption crashes.16 |
| **Concurrency** | Process-based | Multi-threaded | Better connection reuse; no "noisy neighbor" worker blocking.27 |
| **Config Reload** | Can drop connections | Graceful handoff | Updates don't impact user experience.28 |
| **Customization** | Lua (OpenResty) | Rust / Wasm | Higher performance for complex logic (WAF, Bots).32 |

### **4.3 TLS Termination and Cryptography**

Encryption is computationally expensive. The clone must handle TLS 1.3 termination efficiently. Pingora supports **BoringSSL**, the same library used by Google and Cloudflare, ensuring FIPS compliance and access to the latest cryptographic primitives.6 We will configure River to prioritize BoringSSL to leverage assembly-optimized cryptographic routines.

## **5\. Security at the Edge: WAF and DDoS Mitigation**

A simple proxy is not enough; the "Clone" must defend against attacks. The November 18 outage highlighted the importance of the Bot Management layer—and the danger of it failing. We need a security architecture that is robust, high-performance, and isolated from the core proxy stability.

### **5.1 Volumetric DDoS Mitigation: eBPF and XDP**

When an attacker floods a network with millions of packets per second (SYN Flood), handling them in the application layer (Nginx/Pingora) is too slow. The CPU is overwhelmed by context switches and interrupt handling before the application even sees the traffic.

#### **Technology Selection: eBPF / XDP**

We will implement **XDP (eXpress Data Path)** programs using **eBPF (extended Berkeley Packet Filter)**. XDP allows us to run high-performance packet filtering code *inside the network driver*, before the Operating System even allocates a memory buffer (sk\_buff) for the packet.7

**Mechanism:**

1. **Ingress Hook:** The XDP program is attached to the NIC.  
2. **Verdict:** For every incoming packet, the program performs a lookup in a fast hash map (blocklist/allowlist).  
3. **Action:** It returns XDP\_DROP (discard immediately) or XDP\_PASS (send to OS).  
4. **Performance:** This approach can handle tens of millions of packets per second on commodity hardware, effectively neutralizing volumetric attacks.7

**Tooling:** We will use **Cilium** as the orchestration layer for these BPF programs.36 Cilium provides a high-level abstraction for attaching programs to network interfaces and managing the maps that store our blocklists.

### **5.2 Application Layer Security: The WAF**

For Layer 7 attacks (SQL Injection, XSS), we need a Web Application Firewall. The industry standard has long been ModSecurity, but it is notoriously slow and difficult to integrate into modern non-Apache/Nginx environments.37

#### **Technology Selection: Coraza and WebAssembly**

We will deploy **Coraza**, an open-source, enterprise-grade WAF written in Go that is fully compatible with the OWASP Core Rule Set (CRS).37

The Integration Challenge: Embedding a Go-based WAF into a Rust-based Proxy (Pingora) is non-trivial due to different memory models.  
The Solution: Proxy-Wasm.  
We will compile Coraza to WebAssembly (Wasm) using the coraza-proxy-wasm project.39 Pingora (via River) will host a Wasm runtime (using wasmtime).

* **Isolation:** The WAF runs in a sandbox. If a complex regex rule causes the WAF to hang or crash, it does not take down the entire proxy thread. The proxy can fail-open or fail-closed based on policy, avoiding the total blackout seen on Nov 18\.39  
* **Dynamic Updates:** We can push new WAF rules (compiled Wasm modules) to the edge without restarting the binary, enabling rapid response to zero-day threats.

## **6\. Distributed State and Storage: The "Brain" of the Edge**

The November 18 outage was exacerbated by the system's inability to handle the bot management state correctly. A distributed edge network requires a sophisticated storage layer that is fast (for caching) and consistent (for configuration and rate limiting).

### **6.1 Local High-Performance Cache**

At each edge node, we need a Key-Value (KV) store to cache HTML fragments, API responses, and session data. Redis is the standard, but its single-threaded architecture is a bottleneck on modern multi-core servers.

#### **Technology Selection: DragonflyDB**

We will deploy **DragonflyDB**, a modern, multi-threaded replacement for Redis.40 Dragonfly utilizes a shared-nothing architecture that scales vertically across all CPU cores, claiming up to 25x the throughput of Redis.40

* **Efficiency:** It uses a novel "dash-table" indexing structure that is more memory-efficient than Redis, allowing us to store more cache items in the same RAM footprint—critical for edge economics.41  
* **Compatibility:** It speaks the Redis protocol, meaning we can use standard Redis clients in our Pingora/River proxy logic.

### **6.2 Global State Synchronization: CRDTs and Active-Active Replication**

The hardest problem in distributed systems is synchronizing state (e.g., "User X has been rate-limited") across the globe. If we use a single primary database, the latency for a user in Tokyo to check a database in Virginia is unacceptable. We need **Active-Active** replication, where writes can happen anywhere and eventually converge.

#### **Technology Selection: Conflict-Free Replicated Data Types (CRDTs)**

We will utilize **CRDTs** to manage shared state. Unlike strong consistency models (Paxos/Raft) that require locking and consensus (slow), CRDTs allow concurrent updates that are mathematically guaranteed to merge without conflict.8

**Implementation:**

* **Data Structure:** We will use **Loro** or **Automerge** libraries within our application logic to handle complex state (like configuration objects or distributed counters).43  
* Transport Layer: NATS JetStream.  
  To move the CRDT update operations between data centers, we will use NATS JetStream.44 NATS is a high-performance messaging system that supports "Leaf Nodes," allowing edge clusters to function autonomously if the connection to the core is severed, and sync up when connectivity is restored.45  
* **Workflow:**  
  1. Edge Node A updates a rate limit counter (CRDT).  
  2. The operation is published to the local NATS JetStream.  
  3. NATS replicates the message to other regions asynchronously.  
  4. Edge Node B receives the message and merges the operation into its local CRDT view.

**Comparison with Cloudflare:** Cloudflare uses a similar architecture for its KV store.46 By adopting DragonflyDB for local speed and NATS \+ CRDTs for global consistency, we replicate this "Active-Active" capability without the proprietary lock-in.

## **7\. Control Plane and Orchestration: Ensuring Static Stability**

The November 18 outage demonstrated that a centralized Control Plane is a single point of failure. When Cloudflare’s dashboard went down, customers were helpless. Our architecture must adhere to the principle of **Static Stability**: the data plane must be able to operate indefinitely without the control plane.12

### **7.1 Orchestration: Kubernetes at the Edge**

We need to manage the lifecycle of our software (River proxy, DragonflyDB, BIRD) across thousands of nodes. Standard Kubernetes (K8s) is too resource-heavy for smaller edge POPs.

#### **Technology Selection: K3s**

We will utilize **K3s**, a CNCF-certified lightweight Kubernetes distribution designed for the edge.47 K3s packages the necessary components into a single binary of \<100MB, stripping out legacy cloud providers and storage drivers that are unnecessary for bare-metal edge nodes. This reduces the attack surface and resource overhead.

### **7.2 The GitOps Pipeline and Safe Configuration Delivery**

The root cause of the Cloudflare crash was the deployment of a corrupt configuration file. To prevent this, we must eliminate manual/scripted pushes in favor of a **GitOps** model with automated safety checks.

#### **Technology Selection: FluxCD and Flagger**

We will use **FluxCD** to synchronize the state of our edge clusters with a Git repository.48

1. **Audit Trail:** Every configuration change is a Git commit. We know exactly who changed what and when.  
2. **Pull vs. Push:** FluxCD runs *inside* the edge cluster and *pulls* the config. If the central API is down (as in the outage), the edge node simply keeps running its current config. It doesn't depend on a "push" from a central server that might be sending garbage.

The Canary Defense:  
We will implement Progressive Delivery using Flagger.49

* **Scenario:** A new Bot Management config is committed.  
* **Process:** Flagger detects the change. It does *not* update all nodes. It updates a "Canary" deployment receiving 1% of traffic.  
* **Analysis:** Flagger monitors error rates (HTTP 500s).  
* **Outcome:** In the Cloudflare outage scenario, Flagger would have seen the spike in 500 errors on the 1% canary. It would have automatically **halted** the rollout and reverted the canary, preventing the global blackout.14

## **8\. Certificate Management: Automated Trust**

Operating a CDN requires managing millions of SSL/TLS certificates. Manual rotation is impossible.

### **8.1 ACME and Let's Encrypt Integration**

We will integrate the **ACME (Automated Certificate Management Environment)** protocol directly into the River proxy.

* **Mechanism:** When a request arrives for a customer domain (example.com) that does not have a cert, the proxy will hold the connection and initiate an http-01 challenge with **Let's Encrypt**.50  
* **Storage:** The issued certificate is stored in **DragonflyDB** (replicated via NATS) so that other edge nodes can use it immediately without re-validating.  
* **Renewal:** A background process checks certificate validity and auto-renews 30 days before expiration, preventing the "expired cert" outages that plague manual systems.52

## **9\. Comprehensive Architecture Summary**

The following synthesis illustrates the request lifecycle in our proposed "Cloudflare Clone":

1. **Ingress:** A user requests https://client-site.com. BGP Anycast routes them to the nearest Edge Node (e.g., London).  
2. **Network Defense (Kernel):** The NIC receives packets. **Cilium (eBPF/XDP)** programs check the source IP against global threat intelligence lists (synced via NATS). Malicious packets are dropped at the driver level (nanoseconds).  
3. **Proxy (User Space):** Valid packets reach the **River (Pingora)** proxy.  
4. **TLS Termination:** River terminates TLS 1.3 using **BoringSSL**.  
5. **Security Inspection:** River passes the request headers to the **Coraza WAF** running in a **Wasm** sandbox. Coraza checks for SQLi/XSS.  
6. **Cache Lookup:** River checks **DragonflyDB** for a cached response.  
   * *Hit:* Response served immediately.  
   * *Miss:* River proxies the request to the Origin Server.  
7. **State Update:** If the request triggers a rate limit, the counter in DragonflyDB is incremented. The change is broadcast via **NATS JetStream** to other nodes.  
8. **Configuration:** A background **FluxCD** agent ensures the WAF rules and Routing logic match the git repository, having passed a **Flagger** canary test.

## **10\. Implementation Roadmap and Feasibility Analysis**

Building this system is a substantial engineering undertaking. A phased approach is recommended to mitigate risk.

### **Phase 1: The "Iron" Foundation (Months 1-3)**

* **Objective:** Establish the physical network and basic connectivity.  
* **Actions:** Acquire ASN and IP blocks. Deploy bare-metal servers in 3 diverse locations (e.g., US-East, EU-West, APAC-SG). Install Linux (Kernel 6.x) and configure **BIRD** with **Peering Manager**. Verify Anycast routing propagation.

### **Phase 2: The Rust Proxy Prototype (Months 4-6)**

* **Objective:** Replace Nginx with a basic Pingora implementation.  
* **Actions:** deploy **River** proxy. Implement basic HTTP/1.1 and HTTP/2 proxying. Integrate **Let's Encrypt** for auto-TLS. Benchmark throughput against Nginx to verify the threading model benefits.

### **Phase 3: The Security Layer (Months 7-9)**

* **Objective:** Productionize the defensive stack.  
* **Actions:** Write **eBPF** XDP filters for SYN flood protection. Integrate **Coraza WAF** via Wasmtime into River. Perform "Game Day" exercises: simulate a DDoS attack and verify XDP drops packets without CPU spikes.

### **Phase 4: Global State & Orchestration (Months 10-12)**

* **Objective:** Enable "Active-Active" scale.  
* **Actions:** Deploy **DragonflyDB** clusters. Configure **NATS JetStream** leaf nodes for replication. Implement **FluxCD** pipelines with **Flagger** canary logic. Migrate the first beta customer.

## **11\. Conclusion: The Path to Resilience**

The November 18, 2025, outage was a watershed moment for internet infrastructure. It demonstrated that the previous generation of CDN architecture—characterized by C-based monolithic applications, manual or script-based config pushes, and tight coupling between control and data planes—has reached its limits of reliability.

The "Cloudflare Clone" proposed in this report is not a mere imitation; it is an evolution. By embracing **Rust** (Pingora) for memory safety, **eBPF** for high-performance filtering, **CRDTs** for conflict-free global state, and **GitOps** for safe deployment, we architect out the specific failure modes that caused the global blackout. We move from a system that relies on "hope" that a configuration file is correct, to a system that relies on **mathematical guarantees** (CRDTs), **compiler safety** (Rust), and **automated verification** (Canarying). This is the blueprint for the next decade of the resilient internet.

## **12\. Strategic Recommendations for the Engineering Leadership**

1. **Prioritize "Static Stability":** Ensure that every edge node can reboot and serve traffic with zero connection to the central control plane. This is the single most important defense against the "Control Plane Failure" seen on Nov 18\.  
2. **Invest in Rust Expertise:** The shift from C/Nginx to Rust/Pingora is non-trivial. It requires upskilling the engineering team. The long-term payoff in reduced CVEs and higher stability is immense.  
3. **Embrace "Fail-Open" Policies:** In the WAF and Bot Management layers, configure the system such that if the configuration is corrupted or the Wasm module crashes, the system defaults to *allowing* traffic rather than blocking it. Availability must prioritize over security in failure modes.  
4. **Diversify Connectivity:** Do not rely on a single transit provider. Use the automated BGP capabilities of Peering Manager to aggressively peer at IXPs, reducing dependency on any single backbone carrier.

#### **Источники**

1. A major Cloudflare outage took down large parts of the internet \- X, ChatGPT and more were affected, but all recovered now, дата последнего обращения: ноября 19, 2025, [https://www.techradar.com/pro/live/a-cloudflare-outage-is-taking-down-parts-of-the-internet](https://www.techradar.com/pro/live/a-cloudflare-outage-is-taking-down-parts-of-the-internet)  
2. Cloudflare was down, live updates: Huge chunk of internet taken down by outage, дата последнего обращения: ноября 19, 2025, [https://www.tomsguide.com/news/live/cloudfare-outage-november-2025-x-chatgpt](https://www.tomsguide.com/news/live/cloudfare-outage-november-2025-x-chatgpt)  
3. Cloudflare outage explained: What is 'latent bug' that took the internet down, what company said, дата последнего обращения: ноября 19, 2025, [https://timesofindia.indiatimes.com/technology/tech-news/cloudflare-outage-explained-what-is-latent-bug-that-took-the-internet-down-what-company-said/articleshow/125417172.cms](https://timesofindia.indiatimes.com/technology/tech-news/cloudflare-outage-explained-what-is-latent-bug-that-took-the-internet-down-what-company-said/articleshow/125417172.cms)  
4. Outage \- The Cloudflare Blog, дата последнего обращения: ноября 19, 2025, [https://blog.cloudflare.com/tag/outage/](https://blog.cloudflare.com/tag/outage/)  
5. Cloudflare Application Services Products Portfolio, дата последнего обращения: ноября 19, 2025, [https://www.cloudflare.com/application-services/products/](https://www.cloudflare.com/application-services/products/)  
6. Cloudflare Open Sources Pingora, a Rust framework for Developing HTTP Proxies \- InfoQ, дата последнего обращения: ноября 19, 2025, [https://www.infoq.com/news/2024/03/cloudflare-open-sources-pingora/](https://www.infoq.com/news/2024/03/cloudflare-open-sources-pingora/)  
7. Protect from DDoS attacks with eBPF XDP | Dev in the Cloud, дата последнего обращения: ноября 19, 2025, [https://www.srodi.com/posts/ddos-mitication-with-ebpf-xdp/](https://www.srodi.com/posts/ddos-mitication-with-ebpf-xdp/)  
8. Active-Active Redis | Docs, дата последнего обращения: ноября 19, 2025, [https://redis.io/docs/latest/operate/rc/databases/active-active/](https://redis.io/docs/latest/operate/rc/databases/active-active/)  
9. Cloudflare says 'incident now resolved' after outage causes error messages across the internet – as it happened \- The Guardian, дата последнего обращения: ноября 19, 2025, [https://www.theguardian.com/technology/live/2025/nov/18/cloudflare-down-internet-outage-latest-live-news-updates](https://www.theguardian.com/technology/live/2025/nov/18/cloudflare-down-internet-outage-latest-live-news-updates)  
10. Cloudflare Outage November 18, 2025: Complete Analysis of the Internet Infrastructure Failure That Disrupted Thousands of Websites \- ALM Corp, дата последнего обращения: ноября 19, 2025, [https://almcorp.com/blog/cloudflare-outage-november-2025-analysis-protection-guide/](https://almcorp.com/blog/cloudflare-outage-november-2025-analysis-protection-guide/)  
11. Cloudflare outage on November 18, 2025 \- The Cloudflare Blog, дата последнего обращения: ноября 19, 2025, [https://blog.cloudflare.com/18-november-2025-outage/](https://blog.cloudflare.com/18-november-2025-outage/)  
12. Control planes and data planes \- AWS Fault Isolation Boundaries \- AWS Documentation, дата последнего обращения: ноября 19, 2025, [https://docs.aws.amazon.com/whitepapers/latest/aws-fault-isolation-boundaries/control-planes-and-data-planes.html](https://docs.aws.amazon.com/whitepapers/latest/aws-fault-isolation-boundaries/control-planes-and-data-planes.html)  
13. Cloudflare down: What the company's status page has to say on mass outage, дата последнего обращения: ноября 19, 2025, [https://timesofindia.indiatimes.com/technology/tech-news/cloudflare-down-what-the-it-services-company-has-to-say-on-mass-outage/articleshow/125412391.cms](https://timesofindia.indiatimes.com/technology/tech-news/cloudflare-down-what-the-it-services-company-has-to-say-on-mass-outage/articleshow/125412391.cms)  
14. Canary Deployment Strategy Explained \- MOSS, дата последнего обращения: ноября 19, 2025, [https://moss.sh/deployment/canary-deployment-strategy-explained/](https://moss.sh/deployment/canary-deployment-strategy-explained/)  
15. Control plane and data plane \- Reducing the Scope of Impact with Cell-Based Architecture, дата последнего обращения: ноября 19, 2025, [https://docs.aws.amazon.com/wellarchitected/latest/reducing-scope-of-impact-with-cell-based-architecture/control-plane-and-data-plane.html](https://docs.aws.amazon.com/wellarchitected/latest/reducing-scope-of-impact-with-cell-based-architecture/control-plane-and-data-plane.html)  
16. How Cloudflare's Pingora Uses Rust to Replace NGINX: A Game-Changer for Web Performance \- Aarambh Dev Hub, дата последнего обращения: ноября 19, 2025, [https://aarambhdevhub.medium.com/how-cloudflares-pingora-uses-rust-to-replace-nginx-a-game-changer-for-web-performance-e5bf0b1416f2](https://aarambhdevhub.medium.com/how-cloudflares-pingora-uses-rust-to-replace-nginx-a-game-changer-for-web-performance-e5bf0b1416f2)  
17. How does Anycast work? | Cloudflare, дата последнего обращения: ноября 19, 2025, [https://www.cloudflare.com/learning/cdn/glossary/anycast-network/](https://www.cloudflare.com/learning/cdn/glossary/anycast-network/)  
18. Configuring BGP with BIRD 2 on Equinix Metal, дата последнего обращения: ноября 19, 2025, [https://docs.equinix.com/metal/guides/configuring-bgp-with-bird/](https://docs.equinix.com/metal/guides/configuring-bgp-with-bird/)  
19. 2025 Guide to Open-Source Routing Daemons: FRR, BIRD, and ExaBGP \- Bizety, дата последнего обращения: ноября 19, 2025, [https://bizety.com/2025/10/09/2025-guide-to-open-source-routing-daemons-frr-bird-and-exabgp/](https://bizety.com/2025/10/09/2025-guide-to-open-source-routing-daemons-frr-bird-and-exabgp/)  
20. How does anycast work with tcp? \- Server Fault, дата последнего обращения: ноября 19, 2025, [https://serverfault.com/questions/616412/how-does-anycast-work-with-tcp](https://serverfault.com/questions/616412/how-does-anycast-work-with-tcp)  
21. How do you tune an Anycast network for optimal routing? \- Reddit, дата последнего обращения: ноября 19, 2025, [https://www.reddit.com/r/networking/comments/53o154/how\_do\_you\_tune\_an\_anycast\_network\_for\_optimal/](https://www.reddit.com/r/networking/comments/53o154/how_do_you_tune_an_anycast_network_for_optimal/)  
22. Peering Manager, дата последнего обращения: ноября 19, 2025, [https://peering-manager.net/](https://peering-manager.net/)  
23. Peering Manager – automate your peering workflow \- DE-CIX, дата последнего обращения: ноября 19, 2025, [https://www.de-cix.net/en/about-de-cix/news/peering-manager-automate-your-peering-workflow](https://www.de-cix.net/en/about-de-cix/news/peering-manager-automate-your-peering-workflow)  
24. Simplifying Peering with Open Source \- LINX, дата последнего обращения: ноября 19, 2025, [https://www.linx.net/wp-content/uploads/2022/07/LINX118-PeeringManager-GuillaumeMazoyer.pdf](https://www.linx.net/wp-content/uploads/2022/07/LINX118-PeeringManager-GuillaumeMazoyer.pdf)  
25. Peering Manager, дата последнего обращения: ноября 19, 2025, [https://peering-manager.readthedocs.io/](https://peering-manager.readthedocs.io/)  
26. Routing Tools \- Routinator \- NLnet Labs, дата последнего обращения: ноября 19, 2025, [https://nlnetlabs.nl/projects/routing/routinator/](https://nlnetlabs.nl/projects/routing/routinator/)  
27. Pingora is Not an Nginx Replacement | Navendu Pottekkat \- The Open Source Absolutist, дата последнего обращения: ноября 19, 2025, [https://navendu.me/posts/pingora/](https://navendu.me/posts/pingora/)  
28. Open sourcing Pingora: our Rust framework for building programmable network services, дата последнего обращения: ноября 19, 2025, [https://blog.cloudflare.com/pingora-open-source/](https://blog.cloudflare.com/pingora-open-source/)  
29. River Reverse Proxy \- Prossimo \- Memory Safety, дата последнего обращения: ноября 19, 2025, [https://www.memorysafety.org/initiative/reverse-proxy/](https://www.memorysafety.org/initiative/reverse-proxy/)  
30. Announcing River: A High Performance and Memory Safe Reverse Proxy Built on Pingora, дата последнего обращения: ноября 19, 2025, [https://www.memorysafety.org/blog/introducing-river/](https://www.memorysafety.org/blog/introducing-river/)  
31. This repository is the home of the River reverse proxy application, based on the pingora library from Cloudflare. \- GitHub, дата последнего обращения: ноября 19, 2025, [https://github.com/memorysafety/river](https://github.com/memorysafety/river)  
32. CloudFlare Pingora is Now Open Source (in Rust) \- Reddit, дата последнего обращения: ноября 19, 2025, [https://www.reddit.com/r/rust/comments/1b23vhi/cloudflare\_pingora\_is\_now\_open\_source\_in\_rust/](https://www.reddit.com/r/rust/comments/1b23vhi/cloudflare_pingora_is_now_open_source_in_rust/)  
33. Pingora Rust, дата последнего обращения: ноября 19, 2025, [https://www.pingorarust.com/](https://www.pingorarust.com/)  
34. Harnessing eBPF and XDP for DDoS Mitigation \- A Rust Adventure with rust-aya, дата последнего обращения: ноября 19, 2025, [https://dev.to/douglasmakey/harnessing-ebpf-and-xdp-for-ddos-mitigation-a-rust-adventure-with-rust-aya-4k1h](https://dev.to/douglasmakey/harnessing-ebpf-and-xdp-for-ddos-mitigation-a-rust-adventure-with-rust-aya-4k1h)  
35. eBPF XDP monitor and block TLS/SSL encrypted website access \- IPFire Community, дата последнего обращения: ноября 19, 2025, [https://community.ipfire.org/t/ebpf-xdp-monitor-and-block-tls-ssl-encrypted-website-access/13002](https://community.ipfire.org/t/ebpf-xdp-monitor-and-block-tls-ssl-encrypted-website-access/13002)  
36. eBPF Datapath Introduction \- Cilium, дата последнего обращения: ноября 19, 2025, [https://docs.cilium.io/en/stable/network/ebpf/intro.html](https://docs.cilium.io/en/stable/network/ebpf/intro.html)  
37. Talking about ModSecurity and the new Coraza WAF \- OWASP CRS Project, дата последнего обращения: ноября 19, 2025, [https://coreruleset.org/20211222/talking-about-modsecurity-and-the-new-coraza-waf/](https://coreruleset.org/20211222/talking-about-modsecurity-and-the-new-coraza-waf/)  
38. How OWASP Coraza Improved Performance by 100x | by Juan Pablo Tosso \- Medium, дата последнего обращения: ноября 19, 2025, [https://medium.com/@jptosso/how-owasp-coraza-improved-performance-by-100x-38d982371ea9](https://medium.com/@jptosso/how-owasp-coraza-improved-performance-by-100x-38d982371ea9)  
39. proxy-wasm filter based on Coraza WAF \- GitHub, дата последнего обращения: ноября 19, 2025, [https://github.com/corazawaf/coraza-proxy-wasm](https://github.com/corazawaf/coraza-proxy-wasm)  
40. Dragonfly vs. KeyDB Comparison \- SourceForge, дата последнего обращения: ноября 19, 2025, [https://sourceforge.net/software/compare/Dragonfly-DB-vs-KeyDB/](https://sourceforge.net/software/compare/Dragonfly-DB-vs-KeyDB/)  
41. Overcoming Redis Limitations: The Dragonfly DB Approach \- YouTube, дата последнего обращения: ноября 19, 2025, [https://www.youtube.com/watch?v=\_RKlfPpHhKY](https://www.youtube.com/watch?v=_RKlfPpHhKY)  
42. Code (Implementations) \- Conflict-free Replicated Data Types, дата последнего обращения: ноября 19, 2025, [https://crdt.tech/implementations](https://crdt.tech/implementations)  
43. A collection of CRDT benchmarks \- GitHub, дата последнего обращения: ноября 19, 2025, [https://github.com/dmonad/crdt-benchmarks](https://github.com/dmonad/crdt-benchmarks)  
44. Key/Value Store \- NATS Docs, дата последнего обращения: ноября 19, 2025, [https://docs.nats.io/nats-concepts/jetstream/key-value-store](https://docs.nats.io/nats-concepts/jetstream/key-value-store)  
45. JetStream \- NATS Docs, дата последнего обращения: ноября 19, 2025, [https://docs.nats.io/nats-concepts/jetstream](https://docs.nats.io/nats-concepts/jetstream)  
46. Active-Active geo-distribution \- Redis, дата последнего обращения: ноября 19, 2025, [https://redis.io/active-active/](https://redis.io/active-active/)  
47. Unleashing the Power of k3s for Edge Computing: Deploying 3000+ in-store Kubernetes Clusters — Part 1 | by Ryan Gough | JYSK Tech, дата последнего обращения: ноября 19, 2025, [https://jysk.tech/unleashing-the-power-of-k3s-for-edge-computing-deploying-3000-in-store-kubernetes-clusters-part-77ecc5378d31](https://jysk.tech/unleashing-the-power-of-k3s-for-edge-computing-deploying-3000-in-store-kubernetes-clusters-part-77ecc5378d31)  
48. The simplest way to make FluxCD Google Distributed Cloud Edge work like it should, дата последнего обращения: ноября 19, 2025, [https://hoop.dev/blog/the-simplest-way-to-make-fluxcd-google-distributed-cloud-edge-work-like-it-should/](https://hoop.dev/blog/the-simplest-way-to-make-fluxcd-google-distributed-cloud-edge-work-like-it-should/)  
49. Flux CD, дата последнего обращения: ноября 19, 2025, [https://fluxcd.io/](https://fluxcd.io/)  
50. Let's Encrypt, дата последнего обращения: ноября 19, 2025, [https://letsencrypt.org/](https://letsencrypt.org/)  
51. Getting Started \- Let's Encrypt, дата последнего обращения: ноября 19, 2025, [https://letsencrypt.org/getting-started/](https://letsencrypt.org/getting-started/)  
52. How to automate SSL/TLS certificate renewal with Let's Encrypt \- Loadbalancer.org, дата последнего обращения: ноября 19, 2025, [https://www.loadbalancer.org/blog/how-to-automate-ssl-tls-certificate-renewal-with-lets-encrypt/](https://www.loadbalancer.org/blog/how-to-automate-ssl-tls-certificate-renewal-with-lets-encrypt/)