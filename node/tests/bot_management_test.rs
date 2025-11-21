// Bot Management Integration Tests
// Comprehensive tests for Sprint 9 bot detection and management

use aegis_node::bot_management::{BotAction, BotConfig, BotManager, BotVerdict};
use std::thread;
use std::time::Duration;

#[cfg(test)]
mod bot_detection_tests {
    use super::*;

    #[test]
    fn test_search_engine_bots_allowed() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        let search_engines = vec![
            (
                "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
                "Googlebot",
            ),
            (
                "Mozilla/5.0 (compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm)",
                "Bingbot",
            ),
            (
                "Mozilla/5.0 (compatible; Yahoo! Slurp; http://help.yahoo.com/help/us/ysearch/slurp)",
                "Yahoo",
            ),
            (
                "DuckDuckBot/1.0; (+http://duckduckgo.com/duckduckbot.html)",
                "DuckDuckBot",
            ),
            (
                "Mozilla/5.0 (compatible; Baiduspider/2.0; +http://www.baidu.com/search/spider.html)",
                "Baiduspider",
            ),
        ];

        for (user_agent, name) in search_engines {
            let detection = bot_mgr.analyze_request(user_agent, "1.2.3.4");
            assert_eq!(
                detection.verdict,
                BotVerdict::KnownBot,
                "Failed to detect {} as known bot",
                name
            );
            assert_eq!(
                detection.action,
                BotAction::Allow,
                "{} should be allowed",
                name
            );
        }
    }

    #[test]
    fn test_scripted_clients_suspicious() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        let scripted_clients = vec![
            ("python-requests/2.25.1", "Python requests"),
            ("curl/7.68.0", "cURL"),
            ("Wget/1.20.3", "Wget"),
            ("Go-http-client/1.1", "Go HTTP"),
            ("Java/1.8.0_292", "Java HTTP"),
        ];

        for (user_agent, name) in scripted_clients {
            let detection = bot_mgr.analyze_request(user_agent, "192.168.1.100");
            assert_eq!(
                detection.verdict,
                BotVerdict::Suspicious,
                "Failed to detect {} as suspicious",
                name
            );
            assert_eq!(
                detection.action,
                BotAction::Challenge,
                "{} should be challenged",
                name
            );
        }
    }

    #[test]
    fn test_malicious_scrapers_blocked() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request("Scrapy/2.5.0 (+http://scrapy.org)", "10.0.0.1");

        assert_eq!(detection.verdict, BotVerdict::Malicious);
        assert_eq!(detection.action, BotAction::Block);
        assert!(detection.rule_description.unwrap().contains("Scrapy"));
    }

    #[test]
    fn test_legitimate_browsers() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        let browsers = vec![
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/14.1.1 Safari/605.1.15",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:89.0) Gecko/20100101 Firefox/89.0",
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
        ];

        for user_agent in browsers {
            let detection = bot_mgr.analyze_request(user_agent, "192.168.1.50");
            assert_eq!(
                detection.verdict,
                BotVerdict::Human,
                "Legitimate browser marked as bot: {}",
                user_agent
            );
            assert_eq!(detection.action, BotAction::Allow);
        }
    }

    #[test]
    fn test_headless_browsers_suspicious() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        let headless = vec![
            ("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/91.0.4472.124 Safari/537.36", "HeadlessChrome"),
            ("Mozilla/5.0 (Unknown; Linux x86_64) AppleWebKit/538.1 (KHTML, like Gecko) PhantomJS/2.1.1 Safari/538.1", "PhantomJS"),
        ];

        for (user_agent, name) in headless {
            let detection = bot_mgr.analyze_request(user_agent, "192.168.1.60");
            assert_eq!(
                detection.verdict,
                BotVerdict::Suspicious,
                "{} should be suspicious",
                name
            );
        }
    }

    #[test]
    fn test_empty_user_agent_suspicious() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request("", "192.168.1.70");

        assert_eq!(detection.verdict, BotVerdict::Suspicious);
        assert_eq!(detection.action, BotAction::Challenge);
    }
}

#[cfg(test)]
mod whitelist_blacklist_tests {
    use super::*;

    #[test]
    fn test_whitelist_bypass_all_checks() {
        let mut config = BotConfig::default();
        config.whitelist_user_agents.push("TrustedMonitor".to_string());

        let bot_mgr = BotManager::new(config);

        // Even though user-agent contains "scrapy", whitelist takes priority
        let detection = bot_mgr.analyze_request("TrustedMonitor/1.0 Scrapy/2.0", "192.168.1.80");

        assert_eq!(detection.verdict, BotVerdict::Human);
        assert_eq!(detection.action, BotAction::Allow);
        assert!(detection.reason.contains("whitelisted"));
    }

    #[test]
    fn test_blacklist_immediate_block() {
        let mut config = BotConfig::default();
        config.blacklist_user_agents.push("BadActor".to_string());
        config.blacklist_user_agents.push("MaliciousBot".to_string());

        let bot_mgr = BotManager::new(config);

        let detection1 = bot_mgr.analyze_request("BadActor/1.0", "192.168.1.90");
        assert_eq!(detection1.verdict, BotVerdict::Malicious);
        assert_eq!(detection1.action, BotAction::Block);

        let detection2 = bot_mgr.analyze_request("MaliciousBot Scraper", "192.168.1.91");
        assert_eq!(detection2.verdict, BotVerdict::Malicious);
        assert_eq!(detection2.action, BotAction::Block);
    }

    #[test]
    fn test_whitelist_priority_over_blacklist() {
        let mut config = BotConfig::default();
        config.whitelist_user_agents.push("Trusted".to_string());
        config.blacklist_user_agents.push("Trusted".to_string());

        let bot_mgr = BotManager::new(config);

        // Whitelist should take priority
        let detection = bot_mgr.analyze_request("TrustedBot/1.0", "192.168.1.92");
        assert_eq!(detection.verdict, BotVerdict::Human);
        assert_eq!(detection.action, BotAction::Allow);
    }

    #[test]
    fn test_multiple_whitelist_patterns() {
        let mut config = BotConfig::default();
        config.whitelist_user_agents.push("Monitor".to_string());
        config.whitelist_user_agents.push("StatusCheck".to_string());
        config.whitelist_user_agents.push("HealthProbe".to_string());

        let bot_mgr = BotManager::new(config);

        let test_cases = vec![
            "UptimeMonitor/1.0",
            "StatusChecker/2.0",
            "HealthProbe/3.0",
        ];

        for ua in test_cases {
            let detection = bot_mgr.analyze_request(ua, "10.0.0.10");
            assert_eq!(
                detection.verdict,
                BotVerdict::Human,
                "Failed for: {}",
                ua
            );
        }
    }
}

#[cfg(test)]
mod rate_limiting_tests {
    use super::*;

    #[test]
    fn test_rate_limit_tracking() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        let client_ip = "203.0.113.1";

        // Make 10 requests
        for i in 0..10 {
            let detection = bot_mgr.analyze_request(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
                client_ip,
            );

            // First 10 should be allowed
            assert_eq!(
                detection.verdict,
                BotVerdict::Human,
                "Request {} should be allowed",
                i + 1
            );
        }

        // Verify rate tracking
        let stats = bot_mgr.get_rate_limit_stats(client_ip);
        assert!(stats.is_some());
        let (count, _rate) = stats.unwrap();
        assert_eq!(count, 10);
    }

    #[test]
    fn test_rate_limit_reset() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        let client_ip = "203.0.113.2";

        bot_mgr.analyze_request("Mozilla/5.0", client_ip);
        assert!(bot_mgr.get_rate_limit_stats(client_ip).is_some());

        bot_mgr.reset_rate_limit(client_ip);
        assert!(bot_mgr.get_rate_limit_stats(client_ip).is_none());
    }

    #[test]
    fn test_rate_limit_per_ip_isolation() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        // Two different IPs
        bot_mgr.analyze_request("Mozilla/5.0", "203.0.113.3");
        bot_mgr.analyze_request("Mozilla/5.0", "203.0.113.3");
        bot_mgr.analyze_request("Mozilla/5.0", "203.0.113.4");

        let stats1 = bot_mgr.get_rate_limit_stats("203.0.113.3").unwrap();
        let stats2 = bot_mgr.get_rate_limit_stats("203.0.113.4").unwrap();

        assert_eq!(stats1.0, 2); // IP1 made 2 requests
        assert_eq!(stats2.0, 1); // IP2 made 1 request
    }

    #[test]
    fn test_rate_limit_cleared() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        bot_mgr.analyze_request("Mozilla/5.0", "203.0.113.5");
        bot_mgr.analyze_request("Mozilla/5.0", "203.0.113.6");

        assert!(bot_mgr.get_rate_limit_stats("203.0.113.5").is_some());
        assert!(bot_mgr.get_rate_limit_stats("203.0.113.6").is_some());

        bot_mgr.clear_rate_limits();

        assert!(bot_mgr.get_rate_limit_stats("203.0.113.5").is_none());
        assert!(bot_mgr.get_rate_limit_stats("203.0.113.6").is_none());
    }

    #[test]
    fn test_high_rate_triggers_suspicious() {
        let mut config = BotConfig::default();
        config.suspicious_rate_threshold = 5.0; // 5 req/sec
        let bot_mgr = BotManager::new(config);

        let client_ip = "203.0.113.7";

        // Make rapid requests to trigger suspicious rate
        for _ in 0..20 {
            bot_mgr.analyze_request("Mozilla/5.0", client_ip);
        }

        let detection = bot_mgr.analyze_request("Mozilla/5.0", client_ip);

        // Should be marked as suspicious due to high rate
        assert_eq!(detection.verdict, BotVerdict::Suspicious);
        assert_eq!(detection.action, BotAction::Challenge);
        assert!(detection.reason.contains("Suspicious rate"));
    }
}

#[cfg(test)]
mod policy_configuration_tests {
    use super::*;

    #[test]
    fn test_custom_known_bot_action() {
        let mut config = BotConfig::default();
        config.known_bot_action = BotAction::RateLimit;

        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request("Googlebot/2.1", "66.249.66.1");

        assert_eq!(detection.verdict, BotVerdict::KnownBot);
        assert_eq!(detection.action, BotAction::RateLimit);
    }

    #[test]
    fn test_custom_suspicious_action() {
        let mut config = BotConfig::default();
        config.suspicious_action = BotAction::Block;

        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request("curl/7.68.0", "192.168.1.200");

        assert_eq!(detection.verdict, BotVerdict::Suspicious);
        assert_eq!(detection.action, BotAction::Block);
    }

    #[test]
    fn test_custom_malicious_action() {
        let mut config = BotConfig::default();
        config.malicious_action = BotAction::RateLimit;

        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request("Scrapy/2.5.0", "192.168.1.201");

        assert_eq!(detection.verdict, BotVerdict::Malicious);
        assert_eq!(detection.action, BotAction::RateLimit);
    }

    #[test]
    fn test_disabled_bot_management() {
        let mut config = BotConfig::default();
        config.enabled = false;

        let bot_mgr = BotManager::new(config);

        // Even malicious bots should pass through
        let detection = bot_mgr.analyze_request("Scrapy/2.5.0", "192.168.1.202");

        assert_eq!(detection.verdict, BotVerdict::Human);
        assert_eq!(detection.action, BotAction::Allow);
        assert!(detection.reason.contains("disabled"));
    }

    #[test]
    fn test_custom_rate_limit_threshold() {
        let mut config = BotConfig::default();
        config.rate_limit_requests = 5;
        config.rate_limit_window_secs = 10;

        let bot_mgr = BotManager::new(config);

        let client_ip = "203.0.113.8";

        // Make 6 requests (exceeds limit of 5)
        for _ in 0..6 {
            bot_mgr.analyze_request("Mozilla/5.0", client_ip);
        }

        let detection = bot_mgr.analyze_request("Mozilla/5.0", client_ip);

        // Should be blocked for exceeding rate limit
        assert_eq!(detection.verdict, BotVerdict::Malicious);
        assert!(detection.reason.contains("Exceeded 5 requests per 10 seconds"));
    }
}

#[cfg(test)]
mod edge_case_tests {
    use super::*;

    #[test]
    fn test_case_insensitive_bot_detection() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        let test_cases = vec![
            "GOOGLEBOT/2.1",
            "googlebot/2.1",
            "GoogleBot/2.1",
            "GoOgLeBoT/2.1",
        ];

        for ua in test_cases {
            let detection = bot_mgr.analyze_request(ua, "66.249.66.1");
            assert_eq!(
                detection.verdict,
                BotVerdict::KnownBot,
                "Case-insensitive match failed for: {}",
                ua
            );
        }
    }

    #[test]
    fn test_bot_pattern_in_middle_of_string() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request(
            "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
            "66.249.66.1",
        );

        assert_eq!(detection.verdict, BotVerdict::KnownBot);
    }

    #[test]
    fn test_multiple_matching_rules_first_wins() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        // This user-agent matches both "Googlebot" and generic "bot" pattern
        let detection = bot_mgr.analyze_request(
            "Mozilla/5.0 (compatible; Googlebot/2.1)",
            "66.249.66.1",
        );

        // Should match Googlebot rule first (higher priority)
        assert_eq!(detection.verdict, BotVerdict::KnownBot);
        assert_eq!(detection.rule_id, Some(1001));
    }

    #[test]
    fn test_very_long_user_agent() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        let long_ua = format!(
            "{}{}",
            "A".repeat(500),
            " Mozilla/5.0 (compatible; Googlebot/2.1)"
        );

        let detection = bot_mgr.analyze_request(&long_ua, "66.249.66.1");

        assert_eq!(detection.verdict, BotVerdict::KnownBot);
    }

    #[test]
    fn test_special_characters_in_user_agent() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        let special_ua = "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html) [special/chars!@#$%^&*()]";

        let detection = bot_mgr.analyze_request(special_ua, "66.249.66.1");

        assert_eq!(detection.verdict, BotVerdict::KnownBot);
    }

    #[test]
    fn test_unicode_in_user_agent() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        let unicode_ua = "Mozilla/5.0 (compatible; Googlebot/2.1) 日本語 中文 한국어";

        let detection = bot_mgr.analyze_request(unicode_ua, "66.249.66.1");

        assert_eq!(detection.verdict, BotVerdict::KnownBot);
    }
}

#[cfg(test)]
mod concurrent_access_tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_concurrent_bot_detection() {
        let config = BotConfig::default();
        let bot_mgr = Arc::new(BotManager::new(config));

        let mut handles = vec![];

        // Spawn 10 threads making requests concurrently
        for i in 0..10 {
            let mgr = Arc::clone(&bot_mgr);
            let handle = thread::spawn(move || {
                let ip = format!("192.168.1.{}", i);
                for _ in 0..5 {
                    mgr.analyze_request("Mozilla/5.0 (Windows NT 10.0)", &ip);
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify each IP has 5 requests tracked
        for i in 0..10 {
            let ip = format!("192.168.1.{}", i);
            let stats = bot_mgr.get_rate_limit_stats(&ip);
            assert!(stats.is_some());
            let (count, _) = stats.unwrap();
            assert_eq!(count, 5);
        }
    }

    #[test]
    fn test_concurrent_rate_limit_updates() {
        let config = BotConfig::default();
        let bot_mgr = Arc::new(BotManager::new(config));

        let client_ip = "203.0.113.100";

        let mut handles = vec![];

        // Multiple threads updating same IP
        for _ in 0..5 {
            let mgr = Arc::clone(&bot_mgr);
            let ip = client_ip.to_string();
            let handle = thread::spawn(move || {
                for _ in 0..10 {
                    mgr.analyze_request("Mozilla/5.0", &ip);
                    thread::sleep(Duration::from_millis(1));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let stats = bot_mgr.get_rate_limit_stats(client_ip);
        assert!(stats.is_some());
        let (count, _) = stats.unwrap();
        assert_eq!(count, 50); // 5 threads * 10 requests
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;

    #[test]
    fn test_bot_detection_performance() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        let start = std::time::Instant::now();

        // Run 10,000 detections
        for i in 0..10_000 {
            let ip = format!("192.168.{}.{}", i / 256, i % 256);
            bot_mgr.analyze_request("Mozilla/5.0 (Windows NT 10.0; Win64; x64)", &ip);
        }

        let elapsed = start.elapsed();

        // Should complete in reasonable time (< 1 second for 10k requests)
        assert!(
            elapsed.as_secs() < 1,
            "Performance test took too long: {:?}",
            elapsed
        );
    }

    #[test]
    fn test_regex_matching_performance() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        let test_agents = vec![
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            "Mozilla/5.0 (compatible; Googlebot/2.1)",
            "python-requests/2.25.1",
            "curl/7.68.0",
            "Scrapy/2.5.0",
        ];

        let start = std::time::Instant::now();

        for _ in 0..1000 {
            for (i, ua) in test_agents.iter().enumerate() {
                bot_mgr.analyze_request(ua, &format!("10.0.{}.{}", i / 256, i % 256));
            }
        }

        let elapsed = start.elapsed();

        // 5000 regex matches should be fast
        assert!(
            elapsed.as_millis() < 500,
            "Regex matching too slow: {:?}",
            elapsed
        );
    }
}
