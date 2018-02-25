#!/usr/bin/env ruby
require 'bundler'
Bundler.require

doc = Nokogiri::XML(File.open("events.xsd"))
simple_types = doc.root.xpath("//xs:simpleType").inject({}) do |h,e|
    h[e.attributes["name"].value] = if e.children[1].attributes["base"]
        e.children[1].attributes["base"].value
    else
        e.children[1].attributes["memberTypes"].value
    end
    h
end

structs = doc.root.xpath("//xs:element[xs:complexType]").inject({}) do |h,e|
    h[e.attributes["name"].value] = e.xpath(".//xs:attribute").collect do |e2|
        attr_name = e2.attributes["name"].value
        if attr_name == nil
            binding.pry
        end
        attrs = e2.attributes.reject{|k,v| k == "name"}.inject({}) do |h3,e3|
            h3[e3[0]] = e3[1].value; h3
        end
        [attr_name, attrs]
    end
    h
end

doc.root.xpath("//xs:complexType[@name]").inject(structs) do |h,e|
    h[e.attributes["name"].value] ||= []
    h[e.attributes["name"].value] << e.xpath(".//xs:element").inject({}) do |h2,e2|
        begin

            attrs = if e2["ref"]
                h2[e2["ref"]] = ["ref", e2["ref"]]
                attr_name = e2["ref"]
            else
                attr_name = e2.attributes["name"].value
                attrs = e2.attributes.reject{|k,v| k == "name"}.inject({}) do |h3,e3|
                    h3[e3[0]] = e3[1].value; h3
                end

            end

            h2[attr_name] = attrs
        rescue => e
            binding.pry
        end
        h2
    end
    h
end

buf = StringIO.new

structs.each do |struct_name,properties|
    begin
        buf << "pub struct #{struct_name} {"
        buf << "\n"
        properties.each do |prop_name,prop_options|
            begin
            buf << " #{prop_name}: #{prop_options['type']}"
            rescue => e
                binding.pry
            end
            buf << "\n"
        end
        buf << "}"
        buf << "\n"
        # buf << %Q{}}
    rescue => e
        binding.pry
    end
end

# binding.pry

puts buf.string
