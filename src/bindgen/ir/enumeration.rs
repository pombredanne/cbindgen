/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::io::Write;

use syn;

use bindgen::config::{Config, Language};
use bindgen::ir::{AnnotationSet, Cfg, CfgWrite, Documentation, Repr};
use bindgen::rename::{IdentifierType, RenameRule};
use bindgen::utilities::{find_first_some};
use bindgen::writer::{Source, SourceWriter};

#[derive(Debug, Clone)]
pub struct Enum {
    pub name: String,
    pub repr: Repr,
    pub values: Vec<(String, u64, Documentation)>,
    pub cfg: Option<Cfg>,
    pub annotations: AnnotationSet,
    pub documentation: Documentation,
}

impl Enum {
    pub fn load(name: String,
                variants: &Vec<syn::Variant>,
                attrs: &Vec<syn::Attribute>,
                mod_cfg: &Option<Cfg>) -> Result<Enum, String>
    {
        let repr = Repr::load(attrs);

        if repr != Repr::C &&
           repr != Repr::U32 &&
           repr != Repr::U16 &&
           repr != Repr::U8 &&
           repr != Repr::I32 &&
           repr != Repr::I16 &&
           repr != Repr::I8 {
            return Err(format!("enum not marked with a valid repr(prim) or repr(C)"));
        }

        let mut values = Vec::new();
        let mut current = 0;

        for variant in variants {
            match variant.data {
                syn::VariantData::Unit => {
                    match variant.discriminant {
                        Some(syn::ConstExpr::Lit(syn::Lit::Int(i, _))) => {
                            current = i;
                        }
                        Some(_) => {
                            return Err(format!("unsupported discriminant"));
                        }
                        None => { /* okay, we just use current */ }
                    }

                    values.push((variant.ident.to_string(),
                                 current,
                                 Documentation::load(&variant.attrs)));
                    current = current + 1;
                }
                _ => {
                    return Err(format!("unsupported variant"));
                }
            }
        }

        let annotations = AnnotationSet::load(attrs)?;

        if let Some(variants) = annotations.list("enum-trailing-values") {
            for variant in variants {
                values.push((variant, current, Documentation::none()));
                current = current + 1;
            }
        }

        Ok(Enum {
            name: name,
            repr: repr,
            values: values,
            cfg: Cfg::append(mod_cfg, Cfg::load(attrs)),
            annotations: annotations,
            documentation: Documentation::load(attrs),
        })
    }

    pub fn rename_values(&mut self, config: &Config) {
        if config.enumeration.prefix_with_name ||
           self.annotations.bool("prefix-with-name").unwrap_or(false)
        {
            let old = ::std::mem::replace(&mut self.values, Vec::new());
            for (name, value, doc) in old {
                self.values.push((format!("{}_{}", self.name, name), value, doc));
            }
        }

        let rules = [self.annotations.parse_atom::<RenameRule>("rename-all"),
                     config.enumeration.rename_variants];

        if let Some(r) = find_first_some(&rules) {
            self.values = self.values.iter()
                                     .map(|x| (r.apply_to_pascal_case(&x.0,
                                                                      IdentifierType::EnumVariant(self)),
                                               x.1.clone(),
                                               x.2.clone()))
                                     .collect();
        }
    }
}

impl Source for Enum {
    fn write<F: Write>(&self, config: &Config, out: &mut SourceWriter<F>) {
        self.cfg.write_before(config, out);

        self.documentation.write(config, out);

        let size = match self.repr {
            Repr::C => None,
            Repr::U32 => Some("uint32_t"),
            Repr::U16 => Some("uint16_t"),
            Repr::U8 => Some("uint8_t"),
            Repr::I32 => Some("int32_t"),
            Repr::I16 => Some("int16_t"),
            Repr::I8 => Some("int8_t"),
            _ => unreachable!(),
        };

        if config.language == Language::C {
            out.write(&format!("enum {}", self.name));
        } else {
            if let Some(prim) = size {
                out.write(&format!("enum class {} : {}", self.name, prim));
            } else {
                out.write(&format!("enum class {}", self.name));
            }
        }
        out.open_brace();
        for (i, value) in self.values.iter().enumerate() {
            if i != 0 {
                out.new_line()
            }
            value.2.write(config, out);
            out.write(&format!("{} = {},", value.0, value.1));
        }
        if config.enumeration.add_sentinel(&self.annotations) {
            out.new_line();
            out.new_line();
            out.write("Sentinel /* this must be last for serialization purposes. */");
        }
        out.close_brace(true);

        if config.language == Language::C {
            if let Some(prim) = size {
                out.new_line();
                out.write(&format!("typedef {} {};", prim, self.name));
            }
        }

        self.cfg.write_after(config, out);
    }
}
