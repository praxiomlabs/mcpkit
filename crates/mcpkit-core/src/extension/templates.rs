//! Domain-specific extension templates.
//!
//! This module provides ready-to-use extension configurations for common
//! industry domains. These templates encode best practices and standard
//! configurations for specific use cases.
//!
//! # Available Templates
//!
//! - [`healthcare`] - FHIR-compliant healthcare data extensions
//! - [`finance`] - Financial services and trading extensions
//! - [`iot`] - Internet of Things device extensions
//!
//! # Example
//!
//! ```rust
//! use mcpkit_core::extension::templates::healthcare;
//! use mcpkit_core::extension::ExtensionRegistry;
//!
//! // Create a healthcare-compliant server
//! let fhir = healthcare::fhir_extension()
//!     .with_resources(vec!["Patient", "Observation", "Condition"]);
//!
//! let registry = ExtensionRegistry::new()
//!     .register(fhir.build());
//! ```

use super::Extension;

/// Healthcare domain extensions.
///
/// Extensions for healthcare applications, including FHIR compliance,
/// HIPAA considerations, and medical data handling.
pub mod healthcare {
    use super::Extension;
    use serde_json::json;

    /// The FHIR extension namespace.
    pub const FHIR_NAMESPACE: &str = "io.health.fhir";

    /// HL7 FHIR extension version.
    pub const FHIR_VERSION: &str = "1.0.0";

    /// FHIR extension builder.
    ///
    /// Creates an extension for HL7 FHIR-compliant healthcare data handling.
    #[derive(Debug, Clone)]
    pub struct FhirExtensionBuilder {
        fhir_version: String,
        resources: Vec<String>,
        smart_on_fhir: bool,
        audit_logging: bool,
    }

    impl Default for FhirExtensionBuilder {
        fn default() -> Self {
            Self {
                fhir_version: "R4".to_string(),
                resources: Vec::new(),
                smart_on_fhir: false,
                audit_logging: true,
            }
        }
    }

    impl FhirExtensionBuilder {
        /// Create a new FHIR extension builder.
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// Set the FHIR version (e.g., "R4", "R5", "STU3").
        #[must_use]
        pub fn with_fhir_version(mut self, version: impl Into<String>) -> Self {
            self.fhir_version = version.into();
            self
        }

        /// Set the supported FHIR resources.
        #[must_use]
        pub fn with_resources(mut self, resources: Vec<impl Into<String>>) -> Self {
            self.resources = resources.into_iter().map(Into::into).collect();
            self
        }

        /// Enable SMART on FHIR support.
        #[must_use]
        pub fn with_smart_on_fhir(mut self, enabled: bool) -> Self {
            self.smart_on_fhir = enabled;
            self
        }

        /// Configure audit logging.
        #[must_use]
        pub fn with_audit_logging(mut self, enabled: bool) -> Self {
            self.audit_logging = enabled;
            self
        }

        /// Build the extension.
        #[must_use]
        pub fn build(self) -> Extension {
            Extension::new(FHIR_NAMESPACE)
                .with_version(FHIR_VERSION)
                .with_description("HL7 FHIR healthcare data extension")
                .with_config(json!({
                    "fhir_version": self.fhir_version,
                    "resources": self.resources,
                    "smart_on_fhir": self.smart_on_fhir,
                    "audit_logging": self.audit_logging
                }))
        }
    }

    /// Create a FHIR extension builder.
    #[must_use]
    pub fn fhir_extension() -> FhirExtensionBuilder {
        FhirExtensionBuilder::new()
    }

    /// Common FHIR resource types.
    pub mod resources {
        /// Patient demographics and identifiers.
        pub const PATIENT: &str = "Patient";
        /// Clinical observations and measurements.
        pub const OBSERVATION: &str = "Observation";
        /// Clinical conditions and diagnoses.
        pub const CONDITION: &str = "Condition";
        /// Medication prescriptions and orders.
        pub const MEDICATION_REQUEST: &str = "MedicationRequest";
        /// Diagnostic reports and results.
        pub const DIAGNOSTIC_REPORT: &str = "DiagnosticReport";
        /// Clinical encounters and visits.
        pub const ENCOUNTER: &str = "Encounter";
        /// Allergy and intolerance information.
        pub const ALLERGY_INTOLERANCE: &str = "AllergyIntolerance";
        /// Immunization records.
        pub const IMMUNIZATION: &str = "Immunization";
        /// Clinical procedures.
        pub const PROCEDURE: &str = "Procedure";
        /// Care plans and treatment plans.
        pub const CARE_PLAN: &str = "CarePlan";
    }
}

/// Financial services extensions.
///
/// Extensions for financial applications, including market data,
/// trading, regulatory compliance, and risk management.
pub mod finance {
    use super::Extension;
    use serde_json::json;

    /// The financial data extension namespace.
    pub const FINANCE_NAMESPACE: &str = "io.finance.data";

    /// Financial extension version.
    pub const FINANCE_VERSION: &str = "1.0.0";

    /// Financial data extension builder.
    #[derive(Debug, Clone)]
    pub struct FinanceExtensionBuilder {
        data_types: Vec<String>,
        real_time: bool,
        regulatory_compliance: Vec<String>,
        encryption_required: bool,
    }

    impl Default for FinanceExtensionBuilder {
        fn default() -> Self {
            Self {
                data_types: Vec::new(),
                real_time: false,
                regulatory_compliance: Vec::new(),
                encryption_required: true,
            }
        }
    }

    impl FinanceExtensionBuilder {
        /// Create a new finance extension builder.
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// Set supported data types.
        #[must_use]
        pub fn with_data_types(mut self, types: Vec<impl Into<String>>) -> Self {
            self.data_types = types.into_iter().map(Into::into).collect();
            self
        }

        /// Enable real-time data streaming.
        #[must_use]
        pub fn with_real_time(mut self, enabled: bool) -> Self {
            self.real_time = enabled;
            self
        }

        /// Set regulatory compliance frameworks.
        #[must_use]
        pub fn with_regulatory_compliance(mut self, frameworks: Vec<impl Into<String>>) -> Self {
            self.regulatory_compliance = frameworks.into_iter().map(Into::into).collect();
            self
        }

        /// Configure encryption requirement.
        #[must_use]
        pub fn with_encryption_required(mut self, required: bool) -> Self {
            self.encryption_required = required;
            self
        }

        /// Build the extension.
        #[must_use]
        pub fn build(self) -> Extension {
            Extension::new(FINANCE_NAMESPACE)
                .with_version(FINANCE_VERSION)
                .with_description("Financial services data extension")
                .with_config(json!({
                    "data_types": self.data_types,
                    "real_time": self.real_time,
                    "regulatory_compliance": self.regulatory_compliance,
                    "encryption_required": self.encryption_required
                }))
        }
    }

    /// Create a finance extension builder.
    #[must_use]
    pub fn finance_extension() -> FinanceExtensionBuilder {
        FinanceExtensionBuilder::new()
    }

    /// Financial data types.
    pub mod data_types {
        /// Stock/equity quotes and prices.
        pub const EQUITY: &str = "equity";
        /// Fixed income and bond data.
        pub const FIXED_INCOME: &str = "fixed_income";
        /// Foreign exchange rates.
        pub const FX: &str = "fx";
        /// Options and derivatives.
        pub const DERIVATIVES: &str = "derivatives";
        /// Cryptocurrency data.
        pub const CRYPTO: &str = "crypto";
        /// Economic indicators.
        pub const ECONOMIC: &str = "economic";
        /// Corporate actions and events.
        pub const CORPORATE_ACTIONS: &str = "corporate_actions";
    }

    /// Regulatory compliance frameworks.
    pub mod compliance {
        /// Markets in Financial Instruments Directive II.
        pub const MIFID_II: &str = "MiFID II";
        /// Securities and Exchange Commission regulations.
        pub const SEC: &str = "SEC";
        /// General Data Protection Regulation.
        pub const GDPR: &str = "GDPR";
        /// California Consumer Privacy Act.
        pub const CCPA: &str = "CCPA";
        /// Payment Card Industry Data Security Standard.
        pub const PCI_DSS: &str = "PCI-DSS";
        /// Sarbanes-Oxley Act.
        pub const SOX: &str = "SOX";
    }
}

/// `IoT` (Internet of Things) extensions.
///
/// Extensions for `IoT` device management, telemetry,
/// and sensor data handling.
pub mod iot {
    use super::Extension;
    use serde_json::json;

    /// The `IoT` extension namespace.
    pub const IOT_NAMESPACE: &str = "io.iot.devices";

    /// `IoT` extension version.
    pub const IOT_VERSION: &str = "1.0.0";

    /// `IoT` extension builder.
    #[derive(Debug, Clone)]
    pub struct IoTExtensionBuilder {
        device_types: Vec<String>,
        protocols: Vec<String>,
        telemetry_interval_ms: u32,
        buffered_messages: bool,
    }

    impl Default for IoTExtensionBuilder {
        fn default() -> Self {
            Self {
                device_types: Vec::new(),
                protocols: vec!["mqtt".to_string()],
                telemetry_interval_ms: 1000,
                buffered_messages: true,
            }
        }
    }

    impl IoTExtensionBuilder {
        /// Create a new `IoT` extension builder.
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// Set supported device types.
        #[must_use]
        pub fn with_device_types(mut self, types: Vec<impl Into<String>>) -> Self {
            self.device_types = types.into_iter().map(Into::into).collect();
            self
        }

        /// Set supported protocols.
        #[must_use]
        pub fn with_protocols(mut self, protocols: Vec<impl Into<String>>) -> Self {
            self.protocols = protocols.into_iter().map(Into::into).collect();
            self
        }

        /// Set telemetry interval in milliseconds.
        #[must_use]
        pub fn with_telemetry_interval(mut self, interval_ms: u32) -> Self {
            self.telemetry_interval_ms = interval_ms;
            self
        }

        /// Enable message buffering.
        #[must_use]
        pub fn with_buffered_messages(mut self, enabled: bool) -> Self {
            self.buffered_messages = enabled;
            self
        }

        /// Build the extension.
        #[must_use]
        pub fn build(self) -> Extension {
            Extension::new(IOT_NAMESPACE)
                .with_version(IOT_VERSION)
                .with_description("IoT device management extension")
                .with_config(json!({
                    "device_types": self.device_types,
                    "protocols": self.protocols,
                    "telemetry_interval_ms": self.telemetry_interval_ms,
                    "buffered_messages": self.buffered_messages
                }))
        }
    }

    /// Create an `IoT` extension builder.
    #[must_use]
    pub fn iot_extension() -> IoTExtensionBuilder {
        IoTExtensionBuilder::new()
    }

    /// `IoT` device types.
    pub mod device_types {
        /// Temperature and humidity sensors.
        pub const SENSOR: &str = "sensor";
        /// Actuators and controllers.
        pub const ACTUATOR: &str = "actuator";
        /// Gateway devices.
        pub const GATEWAY: &str = "gateway";
        /// Cameras and imaging devices.
        pub const CAMERA: &str = "camera";
        /// Smart meters.
        pub const METER: &str = "meter";
        /// Wearable devices.
        pub const WEARABLE: &str = "wearable";
    }

    /// `IoT` protocols.
    pub mod protocols {
        /// MQTT messaging protocol.
        pub const MQTT: &str = "mqtt";
        /// Constrained Application Protocol.
        pub const COAP: &str = "coap";
        /// HTTP/REST.
        pub const HTTP: &str = "http";
        /// WebSocket.
        pub const WEBSOCKET: &str = "websocket";
        /// Modbus industrial protocol.
        pub const MODBUS: &str = "modbus";
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::ExtensionRegistry;

    #[test]
    fn test_healthcare_fhir() {
        let fhir = healthcare::fhir_extension()
            .with_fhir_version("R4")
            .with_resources(vec![
                healthcare::resources::PATIENT,
                healthcare::resources::OBSERVATION,
            ])
            .with_smart_on_fhir(true)
            .build();

        assert_eq!(fhir.name, healthcare::FHIR_NAMESPACE);
        assert!(fhir.config.is_some());

        let config = fhir.config.unwrap();
        assert_eq!(config["fhir_version"], "R4");
        assert!(config["smart_on_fhir"].as_bool().unwrap());
    }

    #[test]
    fn test_finance_extension() {
        let finance = finance::finance_extension()
            .with_data_types(vec![finance::data_types::EQUITY, finance::data_types::FX])
            .with_real_time(true)
            .with_regulatory_compliance(vec![finance::compliance::MIFID_II])
            .build();

        assert_eq!(finance.name, finance::FINANCE_NAMESPACE);

        let config = finance.config.unwrap();
        assert!(config["real_time"].as_bool().unwrap());
        assert!(config["encryption_required"].as_bool().unwrap());
    }

    #[test]
    fn test_iot_extension() {
        let iot = iot::iot_extension()
            .with_device_types(vec![iot::device_types::SENSOR, iot::device_types::GATEWAY])
            .with_protocols(vec![iot::protocols::MQTT, iot::protocols::COAP])
            .with_telemetry_interval(5000)
            .build();

        assert_eq!(iot.name, iot::IOT_NAMESPACE);

        let config = iot.config.unwrap();
        assert_eq!(config["telemetry_interval_ms"], 5000);
    }

    #[test]
    fn test_registry_with_templates() {
        let registry = ExtensionRegistry::new()
            .register(healthcare::fhir_extension().build())
            .register(finance::finance_extension().build());

        assert!(registry.has(healthcare::FHIR_NAMESPACE));
        assert!(registry.has(finance::FINANCE_NAMESPACE));
        assert_eq!(registry.len(), 2);
    }
}
