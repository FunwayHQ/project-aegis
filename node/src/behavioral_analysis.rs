// Sprint 21: Behavioral Analysis & Trust Scoring
//
// This module implements behavioral analysis to detect bots based on:
// 1. Mouse movement patterns (velocity, acceleration, curvature)
// 2. Keystroke dynamics (inter-key timing, hold duration)
// 3. Scroll behavior (speed, direction changes)
// 4. Touch events (for mobile)
// 5. Timing analysis (time to first interaction)
//
// The behavioral data is combined with TLS fingerprinting (Sprint 19)
// and JavaScript challenges (Sprint 20) into a composite trust score.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Configuration for behavioral analysis thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehavioralConfig {
    /// Minimum number of mouse events for analysis
    pub min_mouse_events: usize,
    /// Minimum number of keystrokes for analysis
    pub min_keystrokes: usize,
    /// Minimum number of scroll events for analysis
    pub min_scroll_events: usize,
    /// Maximum time (ms) for legitimate first interaction
    pub max_first_interaction_ms: u64,
    /// Minimum entropy for natural mouse movement
    pub min_mouse_entropy: f64,
    /// Maximum typing speed (chars/sec) for human
    pub max_human_typing_speed: f64,
    /// Trust score decay per hour
    pub trust_score_decay_per_hour: f64,
}

impl Default for BehavioralConfig {
    fn default() -> Self {
        Self {
            min_mouse_events: 10,
            min_keystrokes: 5,
            min_scroll_events: 3,
            max_first_interaction_ms: 30000, // 30 seconds
            min_mouse_entropy: 2.0,
            max_human_typing_speed: 15.0, // 15 chars/sec is very fast
            trust_score_decay_per_hour: 5.0,
        }
    }
}

/// Mouse movement event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MouseEvent {
    /// X coordinate
    pub x: f64,
    /// Y coordinate
    pub y: f64,
    /// Timestamp (ms since page load)
    pub timestamp: u64,
    /// Event type (move, click, down, up)
    pub event_type: MouseEventType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseEventType {
    Move,
    Click,
    Down,
    Up,
}

/// Keystroke event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeystrokeEvent {
    /// Key code (not the actual key for privacy)
    pub key_code: u32,
    /// Event type (down, up)
    pub event_type: KeyEventType,
    /// Timestamp (ms since page load)
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyEventType {
    Down,
    Up,
}

/// Scroll event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrollEvent {
    /// Scroll position X
    pub scroll_x: f64,
    /// Scroll position Y
    pub scroll_y: f64,
    /// Delta X (scroll amount)
    pub delta_x: f64,
    /// Delta Y (scroll amount)
    pub delta_y: f64,
    /// Timestamp (ms since page load)
    pub timestamp: u64,
}

/// Touch event (for mobile)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TouchEvent {
    /// Touch X coordinate
    pub x: f64,
    /// Touch Y coordinate
    pub y: f64,
    /// Touch pressure (0.0 - 1.0)
    pub pressure: f64,
    /// Contact area radius
    pub radius: f64,
    /// Timestamp (ms since page load)
    pub timestamp: u64,
    /// Event type
    pub event_type: TouchEventType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchEventType {
    Start,
    Move,
    End,
    Cancel,
}

/// Collected behavioral data from client
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BehavioralData {
    /// Page load timestamp (ms since epoch)
    pub page_load_time: u64,
    /// First interaction timestamp (ms since page load)
    pub first_interaction_time: Option<u64>,
    /// Mouse events
    pub mouse_events: Vec<MouseEvent>,
    /// Keystroke events
    pub keystroke_events: Vec<KeystrokeEvent>,
    /// Scroll events
    pub scroll_events: Vec<ScrollEvent>,
    /// Touch events
    pub touch_events: Vec<TouchEvent>,
    /// Visibility changes (tab switches)
    pub visibility_changes: Vec<VisibilityChange>,
    /// Form interactions
    pub form_interactions: Vec<FormInteraction>,
}

/// Visibility change event (tab focus/blur)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisibilityChange {
    /// New visibility state
    pub visible: bool,
    /// Timestamp (ms since page load)
    pub timestamp: u64,
}

/// Form interaction event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormInteraction {
    /// Field identifier (hashed for privacy)
    pub field_hash: String,
    /// Interaction type
    pub interaction_type: FormInteractionType,
    /// Timestamp (ms since page load)
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormInteractionType {
    Focus,
    Blur,
    Input,
    Paste,
}

/// Extracted behavioral features for ML model
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BehavioralFeatures {
    // Mouse features
    pub mouse_event_count: usize,
    pub mouse_avg_velocity: f64,
    pub mouse_velocity_variance: f64,
    pub mouse_avg_acceleration: f64,
    pub mouse_direction_changes: usize,
    pub mouse_entropy: f64,
    pub mouse_straight_line_ratio: f64,
    pub mouse_click_count: usize,
    pub mouse_avg_pause_duration: f64,

    // Keyboard features
    pub keystroke_count: usize,
    pub avg_inter_key_time: f64,
    pub inter_key_time_variance: f64,
    pub avg_key_hold_duration: f64,
    pub key_hold_variance: f64,
    pub typing_speed: f64, // chars per second
    pub paste_events: usize,

    // Scroll features
    pub scroll_event_count: usize,
    pub avg_scroll_speed: f64,
    pub scroll_direction_changes: usize,
    pub scroll_depth: f64, // max scroll Y
    pub scroll_reversals: usize,

    // Touch features (mobile)
    pub touch_event_count: usize,
    pub avg_touch_pressure: f64,
    pub avg_touch_radius: f64,

    // Timing features
    pub time_to_first_interaction: Option<u64>,
    pub session_duration: u64,
    pub visibility_changes: usize,

    // Form features
    pub form_focus_count: usize,
    pub paste_in_forms: usize,
}

/// Behavioral analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehavioralAnalysisResult {
    /// Raw behavioral score (0-100)
    pub score: u8,
    /// Classification verdict
    pub verdict: BehavioralVerdict,
    /// Confidence level (0.0-1.0)
    pub confidence: f64,
    /// Individual feature scores
    pub feature_scores: HashMap<String, f64>,
    /// Detected anomalies
    pub anomalies: Vec<String>,
    /// Extracted features (for debugging)
    pub features: BehavioralFeatures,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BehavioralVerdict {
    Human,
    LikelyHuman,
    Uncertain,
    LikelyBot,
    Bot,
}

/// Composite trust score combining all signals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustScore {
    /// Total trust score (0-100)
    pub score: u8,
    /// TLS fingerprint contribution (0-20)
    pub tls_score: u8,
    /// Challenge completion contribution (0-30)
    pub challenge_score: u8,
    /// Behavioral analysis contribution (0-50)
    pub behavioral_score: u8,
    /// Timestamp when score was calculated
    pub calculated_at: u64,
    /// Session ID for persistence
    pub session_id: String,
    /// IP address hash
    pub ip_hash: String,
    /// Reasons for score adjustments
    pub reasons: Vec<String>,
}

impl TrustScore {
    /// Check if score is above threshold for allowing request
    pub fn is_allowed(&self, threshold: u8) -> bool {
        self.score >= threshold
    }

    /// Get action based on score thresholds
    pub fn recommended_action(&self) -> TrustAction {
        if self.score >= 60 {
            TrustAction::Allow
        } else if self.score >= 30 {
            TrustAction::Challenge
        } else {
            TrustAction::Block
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustAction {
    Allow,
    Challenge,
    Block,
}

/// Behavioral analyzer
pub struct BehavioralAnalyzer {
    config: BehavioralConfig,
}

impl BehavioralAnalyzer {
    /// Create new behavioral analyzer with default config
    pub fn new() -> Self {
        Self {
            config: BehavioralConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: BehavioralConfig) -> Self {
        Self { config }
    }

    /// Extract features from behavioral data
    pub fn extract_features(&self, data: &BehavioralData) -> BehavioralFeatures {
        let mut features = BehavioralFeatures::default();

        // Mouse features
        self.extract_mouse_features(data, &mut features);

        // Keyboard features
        self.extract_keyboard_features(data, &mut features);

        // Scroll features
        self.extract_scroll_features(data, &mut features);

        // Touch features
        self.extract_touch_features(data, &mut features);

        // Timing features
        features.time_to_first_interaction = data.first_interaction_time;
        features.visibility_changes = data.visibility_changes.len();

        // Session duration
        if !data.mouse_events.is_empty() {
            let last_event = data.mouse_events.iter()
                .map(|e| e.timestamp)
                .max()
                .unwrap_or(0);
            features.session_duration = last_event;
        }

        // Form features
        features.form_focus_count = data.form_interactions.iter()
            .filter(|i| i.interaction_type == FormInteractionType::Focus)
            .count();
        features.paste_in_forms = data.form_interactions.iter()
            .filter(|i| i.interaction_type == FormInteractionType::Paste)
            .count();

        features
    }

    /// Extract mouse movement features
    fn extract_mouse_features(&self, data: &BehavioralData, features: &mut BehavioralFeatures) {
        let events = &data.mouse_events;
        features.mouse_event_count = events.len();

        if events.len() < 2 {
            return;
        }

        // Calculate velocities and accelerations
        let mut velocities = Vec::new();
        let mut accelerations = Vec::new();
        let mut direction_changes = 0;
        let mut prev_direction: Option<f64> = None;
        let mut straight_segments = 0;
        let mut total_segments = 0;
        let mut pause_durations = Vec::new();

        for i in 1..events.len() {
            let prev = &events[i - 1];
            let curr = &events[i];

            let dt = (curr.timestamp - prev.timestamp) as f64;
            if dt <= 0.0 {
                continue;
            }

            let dx = curr.x - prev.x;
            let dy = curr.y - prev.y;
            let distance = (dx * dx + dy * dy).sqrt();
            let velocity = distance / dt * 1000.0; // pixels per second

            velocities.push(velocity);

            // Check for pauses (no movement for > 100ms)
            if distance < 1.0 && dt > 100.0 {
                pause_durations.push(dt);
            }

            // Direction change detection
            let direction = dy.atan2(dx);
            if let Some(prev_dir) = prev_direction {
                let angle_diff = (direction - prev_dir).abs();
                if angle_diff > std::f64::consts::PI / 4.0 {
                    direction_changes += 1;
                }
                // Straight line detection (small angle change)
                total_segments += 1;
                if angle_diff < std::f64::consts::PI / 12.0 {
                    straight_segments += 1;
                }
            }
            prev_direction = Some(direction);
        }

        // Velocity statistics
        if !velocities.is_empty() {
            let avg_velocity: f64 = velocities.iter().sum::<f64>() / velocities.len() as f64;
            features.mouse_avg_velocity = avg_velocity;

            let variance: f64 = velocities.iter()
                .map(|v| (v - avg_velocity).powi(2))
                .sum::<f64>() / velocities.len() as f64;
            features.mouse_velocity_variance = variance;
        }

        // Acceleration (velocity changes)
        for i in 1..velocities.len() {
            accelerations.push((velocities[i] - velocities[i - 1]).abs());
        }
        if !accelerations.is_empty() {
            features.mouse_avg_acceleration = accelerations.iter().sum::<f64>() / accelerations.len() as f64;
        }

        features.mouse_direction_changes = direction_changes;

        // Straight line ratio (bots often move in straight lines)
        if total_segments > 0 {
            features.mouse_straight_line_ratio = straight_segments as f64 / total_segments as f64;
        }

        // Entropy calculation (randomness of movement)
        features.mouse_entropy = self.calculate_entropy(&velocities);

        // Click count
        features.mouse_click_count = events.iter()
            .filter(|e| e.event_type == MouseEventType::Click)
            .count();

        // Average pause duration
        if !pause_durations.is_empty() {
            features.mouse_avg_pause_duration = pause_durations.iter().sum::<f64>() / pause_durations.len() as f64;
        }
    }

    /// Extract keyboard features
    fn extract_keyboard_features(&self, data: &BehavioralData, features: &mut BehavioralFeatures) {
        let events = &data.keystroke_events;
        features.keystroke_count = events.len();

        if events.len() < 2 {
            return;
        }

        let mut inter_key_times = Vec::new();
        let mut hold_durations = Vec::new();
        let mut key_down_times: HashMap<u32, u64> = HashMap::new();

        for event in events {
            match event.event_type {
                KeyEventType::Down => {
                    // Record key down time
                    key_down_times.insert(event.key_code, event.timestamp);
                }
                KeyEventType::Up => {
                    // Calculate hold duration
                    if let Some(down_time) = key_down_times.remove(&event.key_code) {
                        let hold = event.timestamp.saturating_sub(down_time);
                        if hold > 0 && hold < 2000 { // Ignore holds > 2 seconds (held key)
                            hold_durations.push(hold as f64);
                        }
                    }
                }
            }
        }

        // Inter-key times (time between consecutive keydowns)
        let key_downs: Vec<_> = events.iter()
            .filter(|e| e.event_type == KeyEventType::Down)
            .collect();

        for i in 1..key_downs.len() {
            let ikt = key_downs[i].timestamp.saturating_sub(key_downs[i - 1].timestamp);
            if ikt > 0 && ikt < 5000 { // Ignore gaps > 5 seconds
                inter_key_times.push(ikt as f64);
            }
        }

        // Statistics
        if !inter_key_times.is_empty() {
            let avg_ikt: f64 = inter_key_times.iter().sum::<f64>() / inter_key_times.len() as f64;
            features.avg_inter_key_time = avg_ikt;

            let variance: f64 = inter_key_times.iter()
                .map(|t| (t - avg_ikt).powi(2))
                .sum::<f64>() / inter_key_times.len() as f64;
            features.inter_key_time_variance = variance;

            // Typing speed (chars per second)
            if avg_ikt > 0.0 {
                features.typing_speed = 1000.0 / avg_ikt;
            }
        }

        if !hold_durations.is_empty() {
            let avg_hold: f64 = hold_durations.iter().sum::<f64>() / hold_durations.len() as f64;
            features.avg_key_hold_duration = avg_hold;

            let variance: f64 = hold_durations.iter()
                .map(|d| (d - avg_hold).powi(2))
                .sum::<f64>() / hold_durations.len() as f64;
            features.key_hold_variance = variance;
        }

        // Paste events (keyboard shortcut detection would be in raw events)
        features.paste_events = 0; // Would need special handling in client
    }

    /// Extract scroll features
    fn extract_scroll_features(&self, data: &BehavioralData, features: &mut BehavioralFeatures) {
        let events = &data.scroll_events;
        features.scroll_event_count = events.len();

        if events.is_empty() {
            return;
        }

        let mut scroll_speeds = Vec::new();
        let mut direction_changes = 0;
        let mut reversals = 0;
        let mut prev_delta_y: Option<f64> = None;
        let mut max_scroll_y = 0.0f64;

        for i in 1..events.len() {
            let prev = &events[i - 1];
            let curr = &events[i];

            let dt = (curr.timestamp - prev.timestamp) as f64;
            if dt <= 0.0 {
                continue;
            }

            let scroll_distance = (curr.delta_x.powi(2) + curr.delta_y.powi(2)).sqrt();
            let speed = scroll_distance / dt * 1000.0;
            scroll_speeds.push(speed);

            // Track max scroll depth
            if curr.scroll_y > max_scroll_y {
                max_scroll_y = curr.scroll_y;
            }

            // Direction changes
            if let Some(prev_dy) = prev_delta_y {
                if prev_dy.signum() != curr.delta_y.signum() && curr.delta_y.abs() > 10.0 {
                    direction_changes += 1;
                    reversals += 1;
                }
            }
            prev_delta_y = Some(curr.delta_y);
        }

        if !scroll_speeds.is_empty() {
            features.avg_scroll_speed = scroll_speeds.iter().sum::<f64>() / scroll_speeds.len() as f64;
        }

        features.scroll_direction_changes = direction_changes;
        features.scroll_depth = max_scroll_y;
        features.scroll_reversals = reversals;
    }

    /// Extract touch features
    fn extract_touch_features(&self, data: &BehavioralData, features: &mut BehavioralFeatures) {
        let events = &data.touch_events;
        features.touch_event_count = events.len();

        if events.is_empty() {
            return;
        }

        let pressures: Vec<f64> = events.iter()
            .filter(|e| e.pressure > 0.0)
            .map(|e| e.pressure)
            .collect();

        if !pressures.is_empty() {
            features.avg_touch_pressure = pressures.iter().sum::<f64>() / pressures.len() as f64;
        }

        let radii: Vec<f64> = events.iter()
            .filter(|e| e.radius > 0.0)
            .map(|e| e.radius)
            .collect();

        if !radii.is_empty() {
            features.avg_touch_radius = radii.iter().sum::<f64>() / radii.len() as f64;
        }
    }

    /// Calculate entropy of a distribution
    fn calculate_entropy(&self, values: &[f64]) -> f64 {
        if values.len() < 2 {
            return 0.0;
        }

        // Bin the values for histogram
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        if (max - min).abs() < f64::EPSILON {
            return 0.0;
        }

        let num_bins = (values.len() as f64).sqrt().ceil() as usize;
        let num_bins = num_bins.max(2).min(20);
        let bin_width = (max - min) / num_bins as f64;

        let mut bins = vec![0usize; num_bins];
        for v in values {
            let bin = ((v - min) / bin_width).floor() as usize;
            let bin = bin.min(num_bins - 1);
            bins[bin] += 1;
        }

        // Calculate entropy
        let total = values.len() as f64;
        let mut entropy = 0.0;
        for count in bins {
            if count > 0 {
                let p = count as f64 / total;
                entropy -= p * p.log2();
            }
        }

        entropy
    }

    /// Analyze behavioral data and produce result
    pub fn analyze(&self, data: &BehavioralData) -> BehavioralAnalysisResult {
        let features = self.extract_features(data);
        let mut score: f64 = 50.0; // Start at neutral
        let mut anomalies = Vec::new();
        let mut feature_scores = HashMap::new();

        // Mouse analysis (up to 20 points)
        let mouse_score = self.score_mouse_behavior(&features, &mut anomalies);
        feature_scores.insert("mouse".to_string(), mouse_score);
        score += mouse_score * 0.2;

        // Keyboard analysis (up to 15 points)
        let keyboard_score = self.score_keyboard_behavior(&features, &mut anomalies);
        feature_scores.insert("keyboard".to_string(), keyboard_score);
        score += keyboard_score * 0.15;

        // Scroll analysis (up to 10 points)
        let scroll_score = self.score_scroll_behavior(&features, &mut anomalies);
        feature_scores.insert("scroll".to_string(), scroll_score);
        score += scroll_score * 0.1;

        // Timing analysis (up to 5 points)
        let timing_score = self.score_timing(&features, &mut anomalies);
        feature_scores.insert("timing".to_string(), timing_score);
        score += timing_score * 0.05;

        // Clamp score to 0-100
        let final_score = score.max(0.0).min(100.0) as u8;

        // Determine verdict
        let verdict = match final_score {
            80..=100 => BehavioralVerdict::Human,
            60..=79 => BehavioralVerdict::LikelyHuman,
            40..=59 => BehavioralVerdict::Uncertain,
            20..=39 => BehavioralVerdict::LikelyBot,
            _ => BehavioralVerdict::Bot,
        };

        // Calculate confidence based on amount of data
        let data_completeness = self.calculate_data_completeness(&features);
        let confidence = data_completeness * (1.0 - (final_score as f64 - 50.0).abs() / 50.0 * 0.3);

        BehavioralAnalysisResult {
            score: final_score,
            verdict,
            confidence: confidence.max(0.0).min(1.0),
            feature_scores,
            anomalies,
            features,
        }
    }

    /// Score mouse behavior
    fn score_mouse_behavior(&self, features: &BehavioralFeatures, anomalies: &mut Vec<String>) -> f64 {
        let mut score: f64 = 50.0;

        // Insufficient data
        if features.mouse_event_count < self.config.min_mouse_events {
            anomalies.push("insufficient_mouse_data".to_string());
            return 0.0;
        }

        // Entropy check (bots have low entropy - predictable movement)
        if features.mouse_entropy < self.config.min_mouse_entropy {
            anomalies.push("low_mouse_entropy".to_string());
            score -= 30.0;
        } else {
            score += (features.mouse_entropy - self.config.min_mouse_entropy).min(3.0) * 10.0;
        }

        // High straight line ratio is suspicious
        if features.mouse_straight_line_ratio > 0.8 {
            anomalies.push("mostly_straight_mouse_movement".to_string());
            score -= 20.0;
        } else if features.mouse_straight_line_ratio < 0.3 {
            score += 15.0;
        }

        // No direction changes is very suspicious
        if features.mouse_direction_changes == 0 && features.mouse_event_count > 20 {
            anomalies.push("no_direction_changes".to_string());
            score -= 25.0;
        } else {
            let change_ratio = features.mouse_direction_changes as f64 / features.mouse_event_count as f64;
            if change_ratio > 0.1 {
                score += 10.0;
            }
        }

        // Very low velocity variance is suspicious (robotic)
        if features.mouse_velocity_variance < 100.0 && features.mouse_event_count > 20 {
            anomalies.push("uniform_mouse_velocity".to_string());
            score -= 15.0;
        }

        // Check for clicks without movement (suspicious)
        if features.mouse_click_count > 0 && features.mouse_event_count < features.mouse_click_count * 5 {
            anomalies.push("clicks_without_movement".to_string());
            score -= 20.0;
        }

        score.max(0.0).min(100.0)
    }

    /// Score keyboard behavior
    fn score_keyboard_behavior(&self, features: &BehavioralFeatures, anomalies: &mut Vec<String>) -> f64 {
        let mut score: f64 = 50.0;

        // Insufficient data
        if features.keystroke_count < self.config.min_keystrokes {
            return 0.0; // No penalty, just no bonus
        }

        // Superhuman typing speed
        if features.typing_speed > self.config.max_human_typing_speed {
            anomalies.push("superhuman_typing_speed".to_string());
            score -= 40.0;
        }

        // Very uniform inter-key timing is suspicious
        if features.inter_key_time_variance < 50.0 && features.keystroke_count > 10 {
            anomalies.push("uniform_typing_rhythm".to_string());
            score -= 30.0;
        } else if features.inter_key_time_variance > 500.0 {
            score += 15.0; // Natural variance
        }

        // Very uniform key hold duration is suspicious
        if features.key_hold_variance < 10.0 && features.keystroke_count > 10 {
            anomalies.push("uniform_key_hold".to_string());
            score -= 20.0;
        }

        // Paste events in forms
        if features.paste_in_forms > 3 {
            anomalies.push("excessive_paste".to_string());
            score -= 15.0;
        }

        score.max(0.0).min(100.0)
    }

    /// Score scroll behavior
    fn score_scroll_behavior(&self, features: &BehavioralFeatures, anomalies: &mut Vec<String>) -> f64 {
        let mut score: f64 = 50.0;

        // Insufficient data
        if features.scroll_event_count < self.config.min_scroll_events {
            return 50.0; // Neutral - not everyone scrolls
        }

        // No scroll reversals is slightly suspicious (bots scroll in one direction)
        if features.scroll_reversals == 0 && features.scroll_event_count > 10 {
            anomalies.push("no_scroll_reversals".to_string());
            score -= 10.0;
        } else if features.scroll_reversals > 2 {
            score += 15.0; // Natural behavior
        }

        // Very uniform scroll speed
        // (Would need variance calculation which we don't have in features)

        // Scroll depth engagement (scrolled to read content)
        if features.scroll_depth > 500.0 {
            score += 10.0;
        }

        score.max(0.0).min(100.0)
    }

    /// Score timing behavior
    fn score_timing(&self, features: &BehavioralFeatures, anomalies: &mut Vec<String>) -> f64 {
        let mut score: f64 = 50.0;

        // Time to first interaction
        if let Some(time_to_first) = features.time_to_first_interaction {
            if time_to_first < 100 {
                // Immediate interaction is suspicious
                anomalies.push("instant_interaction".to_string());
                score -= 30.0;
            } else if time_to_first > self.config.max_first_interaction_ms {
                // Very slow interaction
                score -= 10.0;
            } else if time_to_first > 500 && time_to_first < 5000 {
                // Normal human reaction time
                score += 20.0;
            }
        } else {
            anomalies.push("no_interaction".to_string());
            score -= 20.0;
        }

        // Tab switches (bots rarely switch tabs)
        if features.visibility_changes > 0 {
            score += 10.0;
        }

        score.max(0.0).min(100.0)
    }

    /// Calculate data completeness for confidence
    fn calculate_data_completeness(&self, features: &BehavioralFeatures) -> f64 {
        let mut completeness = 0.0;
        let mut total_weight = 0.0;

        // Mouse data (weight 40%)
        if features.mouse_event_count >= self.config.min_mouse_events {
            completeness += 0.4;
        } else {
            completeness += 0.4 * (features.mouse_event_count as f64 / self.config.min_mouse_events as f64);
        }
        total_weight += 0.4;

        // Keyboard data (weight 30%)
        if features.keystroke_count >= self.config.min_keystrokes {
            completeness += 0.3;
        } else if features.keystroke_count > 0 {
            completeness += 0.3 * (features.keystroke_count as f64 / self.config.min_keystrokes as f64);
        }
        total_weight += 0.3;

        // Scroll data (weight 20%)
        if features.scroll_event_count >= self.config.min_scroll_events {
            completeness += 0.2;
        } else if features.scroll_event_count > 0 {
            completeness += 0.2 * (features.scroll_event_count as f64 / self.config.min_scroll_events as f64);
        }
        total_weight += 0.2;

        // Timing data (weight 10%)
        if features.time_to_first_interaction.is_some() {
            completeness += 0.1;
        }
        total_weight += 0.1;

        completeness / total_weight
    }
}

impl Default for BehavioralAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Trust score calculator combining all signals
pub struct TrustScoreCalculator {
    config: BehavioralConfig,
}

impl TrustScoreCalculator {
    pub fn new() -> Self {
        Self {
            config: BehavioralConfig::default(),
        }
    }

    /// Calculate composite trust score
    pub fn calculate(
        &self,
        tls_fingerprint_valid: bool,
        tls_fingerprint_known: bool,
        challenge_passed: bool,
        challenge_score: Option<u8>,
        behavioral_result: Option<&BehavioralAnalysisResult>,
        client_ip: &str,
        session_id: Option<&str>,
    ) -> TrustScore {
        let mut total_score: u8 = 0;
        let mut reasons = Vec::new();

        // TLS Fingerprint contribution (0-20 points)
        let tls_score = if tls_fingerprint_known && tls_fingerprint_valid {
            reasons.push("known_browser_fingerprint".to_string());
            20
        } else if tls_fingerprint_valid {
            reasons.push("valid_tls_fingerprint".to_string());
            10
        } else {
            reasons.push("unknown_tls_fingerprint".to_string());
            0
        };
        total_score = total_score.saturating_add(tls_score);

        // Challenge contribution (0-30 points)
        let challenge_contribution = if challenge_passed {
            if let Some(score) = challenge_score {
                reasons.push(format!("challenge_passed_score_{}", score));
                // Scale challenge score (0-100) to (0-30)
                (score as u16 * 30 / 100) as u8
            } else {
                reasons.push("challenge_passed".to_string());
                30
            }
        } else {
            0
        };
        total_score = total_score.saturating_add(challenge_contribution);

        // Behavioral contribution (0-50 points)
        let behavioral_contribution = if let Some(result) = behavioral_result {
            // Scale behavioral score (0-100) to (0-50)
            let contrib = (result.score as u16 * 50 / 100) as u8;
            reasons.push(format!("behavioral_score_{}", result.score));
            reasons.extend(result.anomalies.clone());
            contrib
        } else {
            // No behavioral data - neutral
            25
        };
        total_score = total_score.saturating_add(behavioral_contribution);

        // Hash IP and session
        let ip_hash = Self::hash_string(client_ip);
        let session_hash = session_id.map(|s| Self::hash_string(s))
            .unwrap_or_else(|| Self::hash_string(&format!("{}:{}", client_ip, SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs())));

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        TrustScore {
            score: total_score,
            tls_score,
            challenge_score: challenge_contribution,
            behavioral_score: behavioral_contribution,
            calculated_at: now,
            session_id: session_hash,
            ip_hash,
            reasons,
        }
    }

    /// Apply time decay to existing trust score
    pub fn apply_decay(&self, score: &mut TrustScore, hours_elapsed: f64) {
        let decay = (hours_elapsed * self.config.trust_score_decay_per_hour) as u8;
        score.score = score.score.saturating_sub(decay);
        score.behavioral_score = score.behavioral_score.saturating_sub(decay / 2);
    }

    fn hash_string(s: &str) -> String {
        let hash = Sha256::digest(s.as_bytes());
        hex::encode(&hash[..8])
    }
}

impl Default for TrustScoreCalculator {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate JavaScript code for behavioral data collection
pub fn generate_behavior_collection_script() -> String {
    r#"
(function() {
    const AEGIS_BEHAVIOR = {
        pageLoadTime: Date.now(),
        firstInteractionTime: null,
        mouseEvents: [],
        keystrokeEvents: [],
        scrollEvents: [],
        touchEvents: [],
        visibilityChanges: [],
        formInteractions: [],
        maxEvents: 500 // Limit to prevent memory issues
    };

    // Mouse tracking
    let lastMouseTime = 0;
    document.addEventListener('mousemove', function(e) {
        if (AEGIS_BEHAVIOR.mouseEvents.length >= AEGIS_BEHAVIOR.maxEvents) return;
        const now = Date.now() - AEGIS_BEHAVIOR.pageLoadTime;
        // Throttle to max 60 events/sec
        if (now - lastMouseTime < 16) return;
        lastMouseTime = now;

        if (!AEGIS_BEHAVIOR.firstInteractionTime) {
            AEGIS_BEHAVIOR.firstInteractionTime = now;
        }
        AEGIS_BEHAVIOR.mouseEvents.push({
            x: e.clientX,
            y: e.clientY,
            timestamp: now,
            event_type: 'move'
        });
    }, { passive: true });

    document.addEventListener('click', function(e) {
        if (AEGIS_BEHAVIOR.mouseEvents.length >= AEGIS_BEHAVIOR.maxEvents) return;
        const now = Date.now() - AEGIS_BEHAVIOR.pageLoadTime;
        if (!AEGIS_BEHAVIOR.firstInteractionTime) {
            AEGIS_BEHAVIOR.firstInteractionTime = now;
        }
        AEGIS_BEHAVIOR.mouseEvents.push({
            x: e.clientX,
            y: e.clientY,
            timestamp: now,
            event_type: 'click'
        });
    });

    // Keyboard tracking
    document.addEventListener('keydown', function(e) {
        if (AEGIS_BEHAVIOR.keystrokeEvents.length >= AEGIS_BEHAVIOR.maxEvents) return;
        const now = Date.now() - AEGIS_BEHAVIOR.pageLoadTime;
        if (!AEGIS_BEHAVIOR.firstInteractionTime) {
            AEGIS_BEHAVIOR.firstInteractionTime = now;
        }
        AEGIS_BEHAVIOR.keystrokeEvents.push({
            key_code: e.keyCode,
            event_type: 'down',
            timestamp: now
        });
    });

    document.addEventListener('keyup', function(e) {
        if (AEGIS_BEHAVIOR.keystrokeEvents.length >= AEGIS_BEHAVIOR.maxEvents) return;
        const now = Date.now() - AEGIS_BEHAVIOR.pageLoadTime;
        AEGIS_BEHAVIOR.keystrokeEvents.push({
            key_code: e.keyCode,
            event_type: 'up',
            timestamp: now
        });
    });

    // Scroll tracking
    let lastScrollTime = 0;
    document.addEventListener('scroll', function(e) {
        if (AEGIS_BEHAVIOR.scrollEvents.length >= AEGIS_BEHAVIOR.maxEvents) return;
        const now = Date.now() - AEGIS_BEHAVIOR.pageLoadTime;
        // Throttle to max 30 events/sec
        if (now - lastScrollTime < 33) return;
        lastScrollTime = now;

        AEGIS_BEHAVIOR.scrollEvents.push({
            scroll_x: window.scrollX,
            scroll_y: window.scrollY,
            delta_x: 0,
            delta_y: 0,
            timestamp: now
        });
    }, { passive: true });

    // Touch tracking (mobile)
    document.addEventListener('touchstart', function(e) {
        if (AEGIS_BEHAVIOR.touchEvents.length >= AEGIS_BEHAVIOR.maxEvents) return;
        const now = Date.now() - AEGIS_BEHAVIOR.pageLoadTime;
        if (!AEGIS_BEHAVIOR.firstInteractionTime) {
            AEGIS_BEHAVIOR.firstInteractionTime = now;
        }
        for (let touch of e.touches) {
            AEGIS_BEHAVIOR.touchEvents.push({
                x: touch.clientX,
                y: touch.clientY,
                pressure: touch.force || 0,
                radius: touch.radiusX || 0,
                timestamp: now,
                event_type: 'start'
            });
        }
    }, { passive: true });

    document.addEventListener('touchmove', function(e) {
        if (AEGIS_BEHAVIOR.touchEvents.length >= AEGIS_BEHAVIOR.maxEvents) return;
        const now = Date.now() - AEGIS_BEHAVIOR.pageLoadTime;
        for (let touch of e.touches) {
            AEGIS_BEHAVIOR.touchEvents.push({
                x: touch.clientX,
                y: touch.clientY,
                pressure: touch.force || 0,
                radius: touch.radiusX || 0,
                timestamp: now,
                event_type: 'move'
            });
        }
    }, { passive: true });

    // Visibility change tracking
    document.addEventListener('visibilitychange', function() {
        const now = Date.now() - AEGIS_BEHAVIOR.pageLoadTime;
        AEGIS_BEHAVIOR.visibilityChanges.push({
            visible: !document.hidden,
            timestamp: now
        });
    });

    // Form tracking
    document.addEventListener('focus', function(e) {
        if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') {
            const now = Date.now() - AEGIS_BEHAVIOR.pageLoadTime;
            AEGIS_BEHAVIOR.formInteractions.push({
                field_hash: btoa(e.target.name || e.target.id || 'unknown').substring(0, 8),
                interaction_type: 'focus',
                timestamp: now
            });
        }
    }, true);

    document.addEventListener('paste', function(e) {
        if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') {
            const now = Date.now() - AEGIS_BEHAVIOR.pageLoadTime;
            AEGIS_BEHAVIOR.formInteractions.push({
                field_hash: btoa(e.target.name || e.target.id || 'unknown').substring(0, 8),
                interaction_type: 'paste',
                timestamp: now
            });
        }
    }, true);

    // Export function
    window.AEGIS_getBehavioralData = function() {
        return {
            page_load_time: AEGIS_BEHAVIOR.pageLoadTime,
            first_interaction_time: AEGIS_BEHAVIOR.firstInteractionTime,
            mouse_events: AEGIS_BEHAVIOR.mouseEvents,
            keystroke_events: AEGIS_BEHAVIOR.keystrokeEvents,
            scroll_events: AEGIS_BEHAVIOR.scrollEvents,
            touch_events: AEGIS_BEHAVIOR.touchEvents,
            visibility_changes: AEGIS_BEHAVIOR.visibilityChanges,
            form_interactions: AEGIS_BEHAVIOR.formInteractions
        };
    };

    // Submit behavioral data with challenge
    window.AEGIS_submitBehavior = async function(challengeId) {
        const data = window.AEGIS_getBehavioralData();
        const response = await fetch('/aegis/behavior/submit', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                challenge_id: challengeId,
                behavioral_data: data
            })
        });
        return response.json();
    };
})();
"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_human_like_mouse_events() -> Vec<MouseEvent> {
        let mut events = Vec::new();
        let mut x = 100.0;
        let mut y = 100.0;

        for i in 0..50 {
            // Simulate natural mouse movement with curves and pauses
            let angle = (i as f64 * 0.1).sin() * 0.5;
            let speed = 5.0 + (i as f64 * 0.2).cos() * 3.0;
            x += speed * angle.cos();
            y += speed * angle.sin();

            // Add some randomness
            x += (i as f64 * 7.0).sin() * 2.0;
            y += (i as f64 * 11.0).cos() * 2.0;

            events.push(MouseEvent {
                x,
                y,
                timestamp: i as u64 * 50, // ~20 events per second
                event_type: MouseEventType::Move,
            });
        }

        // Add a click
        events.push(MouseEvent {
            x: x + 10.0,
            y: y + 5.0,
            timestamp: 2600,
            event_type: MouseEventType::Click,
        });

        events
    }

    fn create_bot_like_mouse_events() -> Vec<MouseEvent> {
        let mut events = Vec::new();

        // Perfectly straight line movement
        for i in 0..30 {
            events.push(MouseEvent {
                x: 100.0 + i as f64 * 10.0,
                y: 100.0 + i as f64 * 5.0,
                timestamp: i as u64 * 50,
                event_type: MouseEventType::Move,
            });
        }

        events
    }

    #[test]
    fn test_behavioral_analyzer_creation() {
        let analyzer = BehavioralAnalyzer::new();
        assert!(analyzer.config.min_mouse_events > 0);
    }

    #[test]
    fn test_human_like_behavior_analysis() {
        let analyzer = BehavioralAnalyzer::new();

        let data = BehavioralData {
            page_load_time: 1700000000000,
            first_interaction_time: Some(1500),
            mouse_events: create_human_like_mouse_events(),
            keystroke_events: vec![
                KeystrokeEvent { key_code: 65, event_type: KeyEventType::Down, timestamp: 5000 },
                KeystrokeEvent { key_code: 65, event_type: KeyEventType::Up, timestamp: 5120 },
                KeystrokeEvent { key_code: 66, event_type: KeyEventType::Down, timestamp: 5300 },
                KeystrokeEvent { key_code: 66, event_type: KeyEventType::Up, timestamp: 5410 },
                KeystrokeEvent { key_code: 67, event_type: KeyEventType::Down, timestamp: 5600 },
                KeystrokeEvent { key_code: 67, event_type: KeyEventType::Up, timestamp: 5720 },
            ],
            scroll_events: vec![
                ScrollEvent { scroll_x: 0.0, scroll_y: 100.0, delta_x: 0.0, delta_y: 100.0, timestamp: 3000 },
                ScrollEvent { scroll_x: 0.0, scroll_y: 250.0, delta_x: 0.0, delta_y: 150.0, timestamp: 3500 },
                ScrollEvent { scroll_x: 0.0, scroll_y: 500.0, delta_x: 0.0, delta_y: 250.0, timestamp: 4000 },
            ],
            touch_events: vec![],
            visibility_changes: vec![
                VisibilityChange { visible: false, timestamp: 10000 },
                VisibilityChange { visible: true, timestamp: 15000 },
            ],
            form_interactions: vec![],
        };

        let result = analyzer.analyze(&data);

        // Human-like behavior should score well
        assert!(result.score >= 50, "Human-like behavior should score >= 50, got {}", result.score);
        assert!(matches!(result.verdict, BehavioralVerdict::Human | BehavioralVerdict::LikelyHuman | BehavioralVerdict::Uncertain),
            "Expected human-like verdict, got {:?}", result.verdict);
    }

    #[test]
    fn test_bot_like_behavior_analysis() {
        let analyzer = BehavioralAnalyzer::new();

        let data = BehavioralData {
            page_load_time: 1700000000000,
            first_interaction_time: Some(10), // Immediate interaction (suspicious)
            mouse_events: create_bot_like_mouse_events(),
            keystroke_events: vec![
                // Uniform timing (suspicious)
                KeystrokeEvent { key_code: 65, event_type: KeyEventType::Down, timestamp: 1000 },
                KeystrokeEvent { key_code: 65, event_type: KeyEventType::Up, timestamp: 1100 },
                KeystrokeEvent { key_code: 66, event_type: KeyEventType::Down, timestamp: 1200 },
                KeystrokeEvent { key_code: 66, event_type: KeyEventType::Up, timestamp: 1300 },
                KeystrokeEvent { key_code: 67, event_type: KeyEventType::Down, timestamp: 1400 },
                KeystrokeEvent { key_code: 67, event_type: KeyEventType::Up, timestamp: 1500 },
                KeystrokeEvent { key_code: 68, event_type: KeyEventType::Down, timestamp: 1600 },
                KeystrokeEvent { key_code: 68, event_type: KeyEventType::Up, timestamp: 1700 },
                KeystrokeEvent { key_code: 69, event_type: KeyEventType::Down, timestamp: 1800 },
                KeystrokeEvent { key_code: 69, event_type: KeyEventType::Up, timestamp: 1900 },
            ],
            scroll_events: vec![],
            touch_events: vec![],
            visibility_changes: vec![],
            form_interactions: vec![],
        };

        let result = analyzer.analyze(&data);

        // Bot-like behavior should score poorly
        assert!(!result.anomalies.is_empty(), "Should detect anomalies");
    }

    #[test]
    fn test_feature_extraction() {
        let analyzer = BehavioralAnalyzer::new();

        let data = BehavioralData {
            page_load_time: 1700000000000,
            first_interaction_time: Some(1000),
            mouse_events: create_human_like_mouse_events(),
            keystroke_events: vec![],
            scroll_events: vec![
                ScrollEvent { scroll_x: 0.0, scroll_y: 200.0, delta_x: 0.0, delta_y: 200.0, timestamp: 5000 },
            ],
            touch_events: vec![],
            visibility_changes: vec![],
            form_interactions: vec![],
        };

        let features = analyzer.extract_features(&data);

        assert_eq!(features.mouse_event_count, 51);
        assert!(features.mouse_avg_velocity > 0.0);
        assert!(features.mouse_entropy > 0.0);
        assert_eq!(features.mouse_click_count, 1);
        assert_eq!(features.time_to_first_interaction, Some(1000));
    }

    #[test]
    fn test_trust_score_calculation() {
        let calculator = TrustScoreCalculator::new();

        // Full trust: valid TLS, passed challenge, good behavior
        let behavioral_result = BehavioralAnalysisResult {
            score: 80,
            verdict: BehavioralVerdict::Human,
            confidence: 0.9,
            feature_scores: HashMap::new(),
            anomalies: vec![],
            features: BehavioralFeatures::default(),
        };

        let score = calculator.calculate(
            true,  // tls valid
            true,  // tls known
            true,  // challenge passed
            Some(90), // challenge score
            Some(&behavioral_result),
            "192.168.1.1",
            Some("session123"),
        );

        assert!(score.score >= 70, "Full trust should score >= 70, got {}", score.score);
        assert_eq!(score.tls_score, 20);
        assert!(score.challenge_score > 0);
        assert!(score.behavioral_score > 0);
        assert!(score.is_allowed(60));
        assert_eq!(score.recommended_action(), TrustAction::Allow);
    }

    #[test]
    fn test_trust_score_no_challenge() {
        let calculator = TrustScoreCalculator::new();

        let score = calculator.calculate(
            true,  // tls valid
            false, // tls unknown
            false, // challenge not passed
            None,
            None,  // no behavioral data
            "192.168.1.1",
            None,
        );

        // Should get partial score from TLS
        assert!(score.score > 0);
        assert_eq!(score.tls_score, 10);
        assert_eq!(score.challenge_score, 0);
    }

    #[test]
    fn test_entropy_calculation() {
        let analyzer = BehavioralAnalyzer::new();

        // Uniform data - low entropy
        let uniform = vec![5.0, 5.0, 5.0, 5.0, 5.0];
        let uniform_entropy = analyzer.calculate_entropy(&uniform);
        assert!(uniform_entropy < 1.0, "Uniform data should have low entropy");

        // Varied data - higher entropy
        let varied = vec![1.0, 5.0, 2.0, 8.0, 3.0, 9.0, 4.0, 7.0];
        let varied_entropy = analyzer.calculate_entropy(&varied);
        assert!(varied_entropy > uniform_entropy, "Varied data should have higher entropy");
    }

    #[test]
    fn test_behavior_collection_script_generation() {
        let script = generate_behavior_collection_script();

        assert!(script.contains("AEGIS_BEHAVIOR"));
        assert!(script.contains("mousemove"));
        assert!(script.contains("keydown"));
        assert!(script.contains("scroll"));
        assert!(script.contains("touchstart"));
        assert!(script.contains("AEGIS_getBehavioralData"));
        assert!(script.contains("AEGIS_submitBehavior"));
    }

    #[test]
    fn test_trust_action_thresholds() {
        let score_high = TrustScore {
            score: 75,
            tls_score: 20,
            challenge_score: 25,
            behavioral_score: 30,
            calculated_at: 0,
            session_id: "test".to_string(),
            ip_hash: "hash".to_string(),
            reasons: vec![],
        };
        assert_eq!(score_high.recommended_action(), TrustAction::Allow);

        let score_mid = TrustScore {
            score: 45,
            tls_score: 10,
            challenge_score: 15,
            behavioral_score: 20,
            calculated_at: 0,
            session_id: "test".to_string(),
            ip_hash: "hash".to_string(),
            reasons: vec![],
        };
        assert_eq!(score_mid.recommended_action(), TrustAction::Challenge);

        let score_low = TrustScore {
            score: 20,
            tls_score: 5,
            challenge_score: 5,
            behavioral_score: 10,
            calculated_at: 0,
            session_id: "test".to_string(),
            ip_hash: "hash".to_string(),
            reasons: vec![],
        };
        assert_eq!(score_low.recommended_action(), TrustAction::Block);
    }
}
